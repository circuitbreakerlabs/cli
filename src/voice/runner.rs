use super::{
    Error, Result,
    config::Config,
    livekit::{Provider, VoiceSessionProvider, emit},
    protocol::{Audio, Command, Input, MAX_CONTROL_BYTES, QUEUE_FRAMES, VERSION},
};
use crate::{
    protocol_types::{MultiTurnEvaluationRequest, SingleTurnEvaluationRequest},
    tui::{MultiTurnProgressIndicatorMessage, SingleTurnProgressIndicatorMessage},
    websockets::WebSocketConnection,
};
use futures_util::{Sink, SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
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
    path: &Path,
    request: MultiTurnEvaluationRequest,
    progress: Option<mpsc::Sender<MultiTurnProgressIndicatorMessage>>,
) -> Result<Value> {
    let (kind, data) = match &request {
        MultiTurnEvaluationRequest::Standard(data) => ("voice_request", serde_json::to_value(data)),
        MultiTurnEvaluationRequest::Rerun(data) => {
            ("voice_rerun_request", serde_json::to_value(data))
        }
    };
    run_inner(
        websocket,
        path,
        data.map_err(|_| Error::Protocol)?,
        kind,
        request.max_turns(),
        Progress::Multi(progress),
        false,
    )
    .await
}

pub async fn run_single(
    websocket: WebSocketConnection,
    path: &Path,
    request: SingleTurnEvaluationRequest,
    progress: Option<mpsc::Sender<SingleTurnProgressIndicatorMessage>>,
) -> Result<Value> {
    let (kind, data) = match &request {
        SingleTurnEvaluationRequest::Standard(data) => {
            ("voice_request", serde_json::to_value(data))
        }
        SingleTurnEvaluationRequest::Rerun(data) => {
            ("voice_rerun_request", serde_json::to_value(data))
        }
    };
    run_inner(
        websocket,
        path,
        data.map_err(|_| Error::Protocol)?,
        kind,
        1,
        Progress::Single(progress),
        true,
    )
    .await
}

enum Progress {
    Multi(Option<mpsc::Sender<MultiTurnProgressIndicatorMessage>>),
    Single(Option<mpsc::Sender<SingleTurnProgressIndicatorMessage>>),
}

#[allow(clippy::too_many_lines)]
async fn run_inner(
    websocket: WebSocketConnection,
    path: &Path,
    data: Value,
    kind: &str,
    max_turns: usize,
    progress: Progress,
    single: bool,
) -> Result<Value> {
    let config = Config::load(path)?;
    // Resolve secrets and compile hooks before asking the API to start paid work.
    config.context(0, u32::try_from(max_turns).map_err(|_| Error::Protocol)?)?;
    super::hooks::Hooks::new(&config)?;
    if let Some(path) = &config.bootstrap_script {
        super::hooks::Script::load(path)?;
    } else {
        super::config::read_secret(
            config
                .token_env
                .as_deref()
                .ok_or(Error::Configuration("missing token_env"))?,
        )?;
    }
    let target =
        json!({"label":config.label,"transport":"livekit","cli_version":env!("CARGO_PKG_VERSION")});
    let provider: Arc<dyn VoiceSessionProvider> = Arc::new(Provider(config));
    let (mut writer, mut reader) = websocket.split();
    writer
        .send(Message::Text(
            json!({"type":kind,"version":VERSION,"data":data,"target":target})
                .to_string()
                .into(),
        ))
        .await
        .map_err(|_| Error::Transport)?;
    let (output, mut outgoing) = mpsc::channel::<Message>(QUEUE_FRAMES);
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
                    let message = message.ok_or(Error::Transport)?;
                    if let Message::Text(text) = &message {
                        let event: Value = serde_json::from_str(text).map_err(|_| Error::Protocol)?;
                        if event["type"] == "response_end" {
                            let id = i32::try_from(event["session_id"].as_i64().ok_or(Error::Protocol)?).map_err(|_| Error::Protocol)?;
                            let progress_id = sessions.get(&id).map_or(id, |(_, _, conversation_id)| *conversation_id);
                            turn_progress(&progress, progress_id, false, single).await;
                        }
                    }
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
                            let value: Value = serde_json::from_str(&text).map_err(|_| Error::Protocol)?;
                            if text.len() > MAX_CONTROL_BYTES && value["type"] != "voice_result" { return Err(Error::Protocol); }
                            match value.get("type").and_then(Value::as_str) {
                                Some("voice_ready") => {
                                    if value["version"] != VERSION { return Err(Error::Protocol); }
                                    continue;
                                }
                                Some("voice_result") => {
                                    if single {
                                        serde_json::from_value::<
                                            crate::protocol_types::single_turn::SingleTurnResponse,
                                        >(value["data"].clone())
                                        .map_err(|_| Error::Protocol)?;
                                    } else {
                                        serde_json::from_value::<
                                            crate::protocol_types::multi_turn::MultiTurnResponse,
                                        >(value["data"].clone())
                                        .map_err(|_| Error::Protocol)?;
                                    }
                                    return Ok(value["data"].clone());
                                },
                                Some("voice_error") => {
                                    let code = match value.get("code").and_then(Value::as_str) {
                                        Some("authentication") => "authentication",
                                        Some("configuration") => "configuration",
                                        Some("transport" | "overflow") => "transport",
                                        Some("timeout") => "timeout",
                                        Some("synthesis_failed") => "synthesis_failed",
                                        Some("transcription_failed") => "transcription_failed",
                                        Some("invalid_response") => "invalid_response",
                                        Some("invalid_request") => "invalid_request",
                                        Some("not_found") => "not_found",
                                        Some("internal_error") => "internal_error",
                                        _ => "evaluation_failed",
                                    };
                                    return Err(Error::Remote(code.into()));
                                }
                                Some("multi_turn_evaluation_start" | "iteration_start" | "iteration_complete" | "conversation_complete" | "conversation_error") => {
                                    forward_progress(&progress, &value, max_turns, single).await?;
                                    continue;
                                }
                                _ => Input::Control(serde_json::from_value(value).map_err(|_| Error::Protocol)?),
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
                        turn_progress(&progress, progress_id, true, single).await;
                    }
                    if let Input::Control(Command::SessionOpen { conversation_id, max_turns: session_max_turns, timeout_ms, .. }) = input {
                        if sessions.contains_key(&id)
                            || sessions.len() >= 128
                            || timeout_ms == 0
                            || (single && session_max_turns != 1)
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
                                let code = match error {
                                    Error::Authentication => "authentication",
                                    Error::Timeout => "timeout",
                                    Error::Configuration(_) | Error::Hook => "configuration",
                                    Error::Protocol => "invalid_response",
                                    _ => "transport",
                                };
                                let _ = emit(&output, json!({"type":"session_error","session_id":id,"code":code})).await;
                            }
                        });
                    } else if matches!(input, Input::Control(Command::SessionClose { .. } | Command::PlaybackCancel { .. })) {
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
    let send = writer.send(Message::Text(
        json!({"type":"session_error","session_id":session_id,"code":"overflow"})
            .to_string()
            .into(),
    ));
    tokio::select! {
        _ = tokio::signal::ctrl_c() => Err(Error::Cancelled),
        result = tokio::time::timeout(Duration::from_secs(1), send) => match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) | Err(_) => Err(Error::Transport),
        },
    }
}

async fn forward_progress(
    progress: &Progress,
    value: &Value,
    max_turns: usize,
    single: bool,
) -> Result<()> {
    let data = value["data"].clone();
    if single {
        let message = match value["type"].as_str() {
            Some("iteration_start") => SingleTurnProgressIndicatorMessage::IterationStart(
                serde_json::from_value(data).map_err(|_| Error::Protocol)?,
            ),
            Some("iteration_complete") => SingleTurnProgressIndicatorMessage::IterationComplete(
                serde_json::from_value(data).map_err(|_| Error::Protocol)?,
            ),
            Some("conversation_complete") => {
                SingleTurnProgressIndicatorMessage::ConversationComplete(
                    serde_json::from_value(data).map_err(|_| Error::Protocol)?,
                )
            }
            Some("conversation_error") => SingleTurnProgressIndicatorMessage::ConversationError(
                serde_json::from_value(data).map_err(|_| Error::Protocol)?,
            ),
            _ => return Ok(()),
        };
        if let Progress::Single(Some(tx)) = progress {
            let _ = tx.send(message).await;
        }
        return Ok(());
    }
    let message = match value["type"].as_str() {
        Some("multi_turn_evaluation_start") => MultiTurnProgressIndicatorMessage::EvaluationStart {
            conversation_ids: serde_json::from_value(data["conversation_ids"].clone())
                .map_err(|_| Error::Protocol)?,
            max_turns,
        },
        Some("conversation_complete") => MultiTurnProgressIndicatorMessage::ConversationComplete(
            serde_json::from_value(data).map_err(|_| Error::Protocol)?,
        ),
        Some("conversation_error") => MultiTurnProgressIndicatorMessage::ConversationError(
            serde_json::from_value(data).map_err(|_| Error::Protocol)?,
        ),
        _ => return Ok(()),
    };
    if let Progress::Multi(Some(tx)) = progress {
        let _ = tx.send(message).await;
    }
    Ok(())
}

async fn turn_progress(progress: &Progress, id: i32, customer: bool, single: bool) {
    use crate::tui::WaitingFor;
    if single {
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
