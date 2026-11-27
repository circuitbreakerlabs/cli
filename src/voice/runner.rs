use super::{
    Error, Result,
    common_protocol::{ProtocolVersion, SessionErrorCode, VoiceTarget},
    protocol::{
        Audio, Command, EndpointEvent, Input, MAX_CONTROL_BYTES, MultiTurnProgressEvent,
        MultiTurnVoiceResponse, Output, QUEUE_FRAMES, RequestEnvelope, RequestKind, ServerMessage,
        SessionEvent, SingleTurnProgressEvent, SingleTurnVoiceResponse,
    },
    target::PreparedTarget,
};
use crate::{
    protocol_types::{MultiTurnEvaluationRequest, SingleTurnEvaluationRequest},
    tui::{MultiTurnProgressIndicatorMessage, SingleTurnProgressIndicatorMessage},
    websockets::WebSocketConnection,
};
use futures_util::{Sink, SinkExt, StreamExt};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinSet,
};
use tokio_tungstenite::tungstenite::Message;

#[allow(clippy::too_many_lines)] // Central multiplexing loop and its cleanup share task ownership.
pub async fn run(
    websocket: WebSocketConnection,
    target: PreparedTarget,
    request: MultiTurnEvaluationRequest,
    progress: Option<mpsc::Sender<MultiTurnProgressIndicatorMessage>>,
) -> Result<MultiTurnVoiceResponse> {
    let max_turns = request.max_turns();
    let (provider, target) = target_parts(target);
    let request = match request {
        MultiTurnEvaluationRequest::Standard(data) => serde_json::to_string(&RequestEnvelope {
            version: ProtocolVersion::V1,
            kind: RequestKind::VoiceRequest,
            data,
            target,
        }),
        MultiTurnEvaluationRequest::Rerun(data) => serde_json::to_string(&RequestEnvelope {
            version: ProtocolVersion::V1,
            kind: RequestKind::VoiceRerunRequest,
            data,
            target,
        }),
    }
    .map_err(|_| Error::Protocol)?;
    match run_inner(
        websocket,
        provider,
        request,
        max_turns,
        Progress::Multi(progress),
    )
    .await?
    {
        VoiceResult::Multi(result) => Ok(result),
        VoiceResult::Single(_) => Err(Error::Protocol),
    }
}

pub async fn run_single(
    websocket: WebSocketConnection,
    target: PreparedTarget,
    request: SingleTurnEvaluationRequest,
    progress: Option<mpsc::Sender<SingleTurnProgressIndicatorMessage>>,
) -> Result<SingleTurnVoiceResponse> {
    let (provider, target) = target_parts(target);
    let request = match request {
        SingleTurnEvaluationRequest::Standard(data) => serde_json::to_string(&RequestEnvelope {
            version: ProtocolVersion::V1,
            kind: RequestKind::VoiceRequest,
            data,
            target,
        }),
        SingleTurnEvaluationRequest::Rerun(data) => serde_json::to_string(&RequestEnvelope {
            version: ProtocolVersion::V1,
            kind: RequestKind::VoiceRerunRequest,
            data,
            target,
        }),
    }
    .map_err(|_| Error::Protocol)?;
    match run_inner(websocket, provider, request, 1, Progress::Single(progress)).await? {
        VoiceResult::Single(result) => Ok(result),
        VoiceResult::Multi(_) => Err(Error::Protocol),
    }
}

fn target_parts(
    target: PreparedTarget,
) -> (
    std::sync::Arc<dyn super::session::VoiceSessionProvider>,
    VoiceTarget,
) {
    let PreparedTarget { provider, metadata } = target;
    (
        provider,
        VoiceTarget {
            label: metadata.label,
            transport: metadata.transport,
            cli_version: env!("CARGO_PKG_VERSION").into(),
        },
    )
}

enum Progress {
    Multi(Option<mpsc::Sender<MultiTurnProgressIndicatorMessage>>),
    Single(Option<mpsc::Sender<SingleTurnProgressIndicatorMessage>>),
}

enum VoiceResult {
    Single(SingleTurnVoiceResponse),
    Multi(MultiTurnVoiceResponse),
}

enum Incoming {
    Ready(ProtocolVersion),
    Error(super::common_protocol::VoiceErrorCode),
    Result(VoiceResult),
    SingleProgress(SingleTurnProgressEvent),
    MultiProgress(MultiTurnProgressEvent),
    Command(Command),
}

#[allow(clippy::too_many_lines)]
async fn run_inner(
    websocket: WebSocketConnection,
    provider: std::sync::Arc<dyn super::session::VoiceSessionProvider>,
    request: String,
    max_turns: usize,
    progress: Progress,
) -> Result<VoiceResult> {
    let (mut writer, mut reader) = websocket.split();
    writer
        .send(Message::Text(request.into()))
        .await
        .map_err(|_| Error::Transport)?;
    let (output, mut outgoing) = mpsc::channel::<Output>(QUEUE_FRAMES);
    let mut sessions = HashMap::<i32, (mpsc::Sender<Input>, watch::Sender<bool>, i32)>::new();
    let mut overflowed_sessions = HashSet::new();
    let mut tasks = JoinSet::new();
    let result = async {
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => return Err(Error::Cancelled),
                item = tasks.join_next(), if !tasks.is_empty() => {
                    if item.is_some_and(|result| result.is_err()) { return Err(Error::Transport); }
                }
                message = outgoing.recv() => {
                    let message = match message.ok_or(Error::Transport)? {
                        Output::Event(event) => {
                            if let SessionEvent::ResponseEnd { session_id: id, .. } = &event {
                            let progress_id = sessions.get(id).map_or(*id, |(_, _, conversation_id)| *conversation_id);
                            turn_progress(&progress, progress_id, false).await;
                            }
                            Message::Text(serde_json::to_string(&event).map_err(|_| Error::Protocol)?.into())
                        }
                        Output::Audio(audio) => Message::Binary(audio.encode()?.into()),
                    };
                    writer.send(message).await.map_err(|_| Error::Transport)?;
                }
                message = reader.next() => {
                    let message = message.ok_or(Error::Transport)?.map_err(|_| Error::Transport)?;
                    let input = match message {
                        Message::Ping(data) => { writer.send(Message::Pong(data)).await.map_err(|_| Error::Transport)?; continue; }
                        Message::Pong(_) => continue,
                        Message::Close(_) => return Err(Error::Transport),
                        Message::Binary(bytes) => Input::Audio(Audio::decode(&bytes)?),
                        Message::Text(text) => {
                            if text.len() > 16 * 1024 * 1024 { return Err(Error::Protocol); }
                            let incoming = parse_incoming(&text, &progress)?;
                            if text.len() > MAX_CONTROL_BYTES && !matches!(incoming, Incoming::Result(_)) {
                                return Err(Error::Protocol);
                            }
                            match incoming {
                                Incoming::Ready(ProtocolVersion::V1) => continue,
                                Incoming::Ready(_) => return Err(Error::Protocol),
                                Incoming::Error(code) => return Err(Error::Remote(code)),
                                Incoming::Result(result) => return Ok(result),
                                Incoming::SingleProgress(event) => {
                                    forward_single_progress(&progress, event).await;
                                    continue;
                                }
                                Incoming::MultiProgress(event) => {
                                    forward_multi_progress(&progress, event, max_turns).await;
                                    continue;
                                }
                                Incoming::Command(command) => {
                                    command.validate()?;
                                    Input::Control(command)
                                }
                            }
                        }
                        Message::Frame(_) => return Err(Error::Protocol),
                    };
                    let id = match &input { Input::Audio(audio) => audio.header.session_id, Input::Control(command) => command.session_id() };
                    if overflowed_sessions.contains(&id) {
                        if matches!(input, Input::Control(Command::SessionClose { .. })) {
                            overflowed_sessions.remove(&id);
                            if let Some((_, stop, _)) = sessions.remove(&id) {
                                let _ = stop.send(true);
                            }
                        } else if matches!(input, Input::Control(Command::SessionOpen { .. })) {
                            return Err(Error::Protocol);
                        }
                        continue;
                    }
                    if let Input::Control(Command::UtteranceStart { session_id, .. }) = &input {
                        let progress_id = sessions
                            .get(session_id)
                            .map_or(*session_id, |(_, _, conversation_id)| *conversation_id);
                        turn_progress(&progress, progress_id, true).await;
                    }
                    if let Input::Control(Command::SessionOpen { conversation_id, max_turns: session_max_turns, timeout_ms, .. }) = input {
                        if sessions.contains_key(&id)
                            || sessions.len() >= 128
                            || timeout_ms == 0
                            || (matches!(progress, Progress::Single(_)) && session_max_turns != 1)
                        {
                            return Err(Error::Protocol);
                        }
                        let (tx, rx) = mpsc::channel(QUEUE_FRAMES + 2);
                        let (session_stop, stop_rx) = watch::channel(false);
                        sessions.insert(id, (tx, session_stop, conversation_id.unwrap_or(id)));
                        let provider = provider.clone();
                        let output = output.clone();
                        tasks.spawn(async move {
                            let execution = async {
                                let session = tokio::time::timeout(Duration::from_millis(timeout_ms.min(3_300_000)), provider.connect(id, session_max_turns)).await.map_err(|_| Error::Timeout)??;
                                session.run(rx, output.clone(), stop_rx).await
                            };
                            let result = execution.await;
                            if let Err(error) = result {
                                tracing::warn!(session_id = id, error = %error, "Voice session failed");
                                let code = session_error_code(&error);
                                let _ = output.send(Output::Event(SessionEvent::SessionError { session_id: id, code })).await;
                            }
                        });
                    } else if matches!(input, Input::Control(Command::SessionClose { .. } | Command::PlaybackCancel { .. })) {
                        let reason = if matches!(input, Input::Control(Command::SessionClose { .. })) { "session_close" } else { "playback_cancel" };
                        tracing::info!(session_id = id, reason, "voice_diag: API requested stop");
                        if let Some((_, stop, _)) = sessions.remove(&id) { let _ = stop.send(true); }
                    } else {
                        let Some((tx, session_stop, _)) = sessions.get(&id) else { return Err(Error::Protocol); };
                        if tx.try_send(input).is_err() {
                            let _ = session_stop.send(true);
                            overflowed_sessions.insert(id);
                            send_overflow(&mut writer, id).await?;
                        }
                    }
                }
            }
        }
    }.await;
    tracing::info!(error = ?result.as_ref().err(), remaining_sessions = sessions.len(), "voice_diag: evaluation ended; stopping remaining sessions");
    for (_, session_stop, _) in sessions.values() {
        let _ = session_stop.send(true);
    }
    // Continue draining output while sessions close so cleanup cannot deadlock on
    // a full writer queue after the evaluator has returned its final result.
    let cleanup = async {
        while !tasks.is_empty() {
            tokio::select! { _ = tasks.join_next() => {}, _ = outgoing.recv() => {} }
        }
    };
    if tokio::time::timeout(Duration::from_secs(8), cleanup)
        .await
        .is_err()
    {
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    }
    let _ = writer.close().await;
    match progress {
        Progress::Multi(Some(tx)) => {
            let _ = tx
                .send(MultiTurnProgressIndicatorMessage::EvaluationComplete)
                .await;
        }
        Progress::Single(Some(tx)) => {
            let _ = tx
                .send(SingleTurnProgressIndicatorMessage::EvaluationComplete)
                .await;
        }
        Progress::Multi(None) | Progress::Single(None) => {}
    }
    result
}

async fn send_overflow<S>(writer: &mut S, session_id: i32) -> Result<()>
where
    S: Sink<Message> + Unpin,
{
    let message = serde_json::to_string(&SessionEvent::SessionError {
        session_id,
        code: SessionErrorCode::Overflow,
    })
    .map_err(|_| Error::Protocol)?;
    let send = writer.send(Message::Text(message.into()));
    tokio::select! {
        _ = tokio::signal::ctrl_c() => Err(Error::Cancelled),
        result = tokio::time::timeout(Duration::from_secs(1), send) => match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) | Err(_) => Err(Error::Transport),
        },
    }
}

fn parse_incoming(text: &str, progress: &Progress) -> Result<Incoming> {
    match progress {
        Progress::Single(_) => {
            let message: ServerMessage<SingleTurnVoiceResponse, SingleTurnProgressEvent> =
                serde_json::from_str(text).map_err(|_| Error::Protocol)?;
            Ok(match message {
                ServerMessage::Endpoint(event) => {
                    event.validate()?;
                    match event {
                        EndpointEvent::VoiceReady { version } => Incoming::Ready(version),
                        EndpointEvent::VoiceError { code, .. } => Incoming::Error(code),
                        EndpointEvent::VoiceResult { data } => {
                            Incoming::Result(VoiceResult::Single(data))
                        }
                    }
                }
                ServerMessage::Progress(event) => Incoming::SingleProgress(event),
                ServerMessage::Command(command) => Incoming::Command(command),
            })
        }
        Progress::Multi(_) => {
            let message: ServerMessage<MultiTurnVoiceResponse, MultiTurnProgressEvent> =
                serde_json::from_str(text).map_err(|_| Error::Protocol)?;
            Ok(match message {
                ServerMessage::Endpoint(event) => {
                    event.validate()?;
                    match event {
                        EndpointEvent::VoiceReady { version } => Incoming::Ready(version),
                        EndpointEvent::VoiceError { code, .. } => Incoming::Error(code),
                        EndpointEvent::VoiceResult { data } => {
                            Incoming::Result(VoiceResult::Multi(data))
                        }
                    }
                }
                ServerMessage::Progress(event) => Incoming::MultiProgress(event),
                ServerMessage::Command(command) => Incoming::Command(command),
            })
        }
    }
}

async fn forward_single_progress(progress: &Progress, event: SingleTurnProgressEvent) {
    let message = match event {
        SingleTurnProgressEvent::IterationStart(data) => {
            SingleTurnProgressIndicatorMessage::IterationStart(data)
        }
        SingleTurnProgressEvent::IterationComplete(data) => {
            SingleTurnProgressIndicatorMessage::IterationComplete(data)
        }
        SingleTurnProgressEvent::ConversationComplete(data) => {
            SingleTurnProgressIndicatorMessage::ConversationComplete(data)
        }
        SingleTurnProgressEvent::ConversationError(data) => {
            SingleTurnProgressIndicatorMessage::ConversationError(data)
        }
    };
    if let Progress::Single(Some(tx)) = progress {
        let _ = tx.send(message).await;
    }
}

async fn forward_multi_progress(
    progress: &Progress,
    event: MultiTurnProgressEvent,
    max_turns: usize,
) {
    let message = match event {
        MultiTurnProgressEvent::MultiTurnEvaluationStart(data) => {
            MultiTurnProgressIndicatorMessage::EvaluationStart {
                conversation_ids: data.conversation_ids,
                max_turns,
            }
        }
        MultiTurnProgressEvent::ConversationComplete(data) => {
            MultiTurnProgressIndicatorMessage::ConversationComplete(data)
        }
        MultiTurnProgressEvent::ConversationError(data) => {
            MultiTurnProgressIndicatorMessage::ConversationError(data)
        }
    };
    if let Progress::Multi(Some(tx)) = progress {
        let _ = tx.send(message).await;
    }
}

fn session_error_code(error: &Error) -> SessionErrorCode {
    match error {
        Error::Authentication => SessionErrorCode::Authentication,
        Error::Timeout => SessionErrorCode::Timeout,
        Error::Configuration(_) | Error::Hook => SessionErrorCode::Configuration,
        Error::Protocol => SessionErrorCode::InvalidResponse,
        Error::Cancelled => SessionErrorCode::Cancelled,
        Error::Transport | Error::TransportStage(_) | Error::Remote(_) => {
            SessionErrorCode::Transport
        }
    }
}

async fn turn_progress(progress: &Progress, id: i32, customer: bool) {
    use crate::tui::WaitingFor;
    if matches!(progress, Progress::Single(_)) {
        if let Progress::Single(Some(tx)) = progress {
            let _ = tx
                .send(SingleTurnProgressIndicatorMessage::WaitingFor {
                    conversation_id: id,
                    waiting_for: if customer {
                        WaitingFor::Provider
                    } else {
                        WaitingFor::API
                    },
                })
                .await;
        }
    } else if let Progress::Multi(Some(tx)) = progress {
        let _ = tx
            .send(MultiTurnProgressIndicatorMessage::ConversationTurn {
                conversation_id: id,
            })
            .await;
        let _ = tx
            .send(MultiTurnProgressIndicatorMessage::WaitingFor {
                conversation_id: id,
                waiting_for: if customer {
                    WaitingFor::Provider
                } else {
                    WaitingFor::API
                },
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::{
        io,
        pin::Pin,
        task::{Context, Poll},
    };

    #[derive(Default)]
    struct RecordingSink {
        messages: Vec<Message>,
    }

    impl Sink<Message> for RecordingSink {
        type Error = io::Error;

        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(
            mut self: Pin<&mut Self>,
            item: Message,
        ) -> std::result::Result<(), Self::Error> {
            self.messages.push(item);
            Ok(())
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    struct StalledSink;

    impl Sink<Message> for StalledSink {
        type Error = io::Error;

        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Pending
        }

        fn start_send(
            self: Pin<&mut Self>,
            _item: Message,
        ) -> std::result::Result<(), Self::Error> {
            unreachable!("a stalled sink never accepts messages")
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Pending
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Pending
        }
    }

    #[tokio::test]
    async fn overflow_error_bypasses_full_outgoing_queue() {
        let (outgoing, mut queued) = mpsc::channel(1);
        outgoing
            .try_send(Message::Text("queued".to_owned().into()))
            .expect("the outgoing queue should accept its first message");
        let mut writer = RecordingSink::default();

        send_overflow(&mut writer, 7)
            .await
            .expect("the direct writer should accept the overflow error");

        assert_eq!(
            queued.try_recv().unwrap(),
            Message::Text("queued".to_owned().into())
        );
        let [Message::Text(message)] = writer.messages.as_slice() else {
            panic!("expected one direct overflow message");
        };
        assert_eq!(
            serde_json::from_str::<Value>(message).unwrap(),
            json!({"type":"session_error","session_id":7,"code":"overflow"})
        );
    }

    #[tokio::test]
    async fn overflow_error_times_out_when_writer_stalls() {
        let mut writer = StalledSink;

        assert!(matches!(
            send_overflow(&mut writer, 9).await,
            Err(Error::Transport)
        ));
    }
}
