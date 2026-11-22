use super::{
    Error, Result,
    common_protocol::{ProtocolVersion, SessionErrorCode, VoiceTarget},
    multiparty_protocol::{
        Audio, Command, EndpointEvent, Input, MAX_CONTROL_BYTES, Output, QUEUE_FRAMES,
        RequestEnvelope, RequestKind, ServerMessage, SessionEvent,
    },
    session::MultipartySessionSpec,
    target::PreparedTarget,
};
use crate::{
    protocol_types::multi_party::{MultiPartyEvaluationRequest, MultiPartyVoiceResponse},
    tui::{WaitingFor, multiparty::MultiPartyProgressIndicatorMessage},
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

struct SessionHandle {
    input: mpsc::Sender<Input>,
    stop: watch::Sender<bool>,
    conversation_id: i32,
}

#[allow(clippy::too_many_lines)] // One select loop owns multiplexing and cleanup.
pub async fn run_multiparty(
    websocket: WebSocketConnection,
    target: PreparedTarget,
    request: MultiPartyEvaluationRequest,
    progress: Option<mpsc::Sender<MultiPartyProgressIndicatorMessage>>,
) -> Result<MultiPartyVoiceResponse> {
    let PreparedTarget { provider, metadata } = target;
    let target = VoiceTarget {
        label: metadata.label,
        transport: metadata.transport,
        cli_version: env!("CARGO_PKG_VERSION").into(),
    };
    let request = match request {
        MultiPartyEvaluationRequest::Standard(data) => serde_json::to_string(&RequestEnvelope {
            version: ProtocolVersion::V2,
            kind: RequestKind::MultipartyVoiceRequest,
            data,
            target,
        }),
        MultiPartyEvaluationRequest::Rerun(data) => serde_json::to_string(&RequestEnvelope {
            version: ProtocolVersion::V2,
            kind: RequestKind::MultipartyVoiceRerunRequest,
            data,
            target,
        }),
    }
    .map_err(|_| Error::Protocol)?;
    let (mut writer, mut reader) = websocket.split();
    writer
        .send(Message::Text(request.into()))
        .await
        .map_err(|_| Error::Transport)?;

    let (output, mut outgoing) = mpsc::channel::<Output>(QUEUE_FRAMES);
    let mut sessions = HashMap::<i32, SessionHandle>::new();
    let mut finished_sessions = HashSet::<i32>::new();
    let mut overflowed_sessions = HashSet::<i32>::new();
    let mut tasks = JoinSet::<(i32, Result<()>)>::new();
    let result = async {
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => return Err(Error::Cancelled),
                completed = tasks.join_next(), if !tasks.is_empty() => {
                    let Some(completed) = completed else { continue };
                    let (id, session_result) = completed.map_err(|_| Error::Transport)?;
                    sessions.remove(&id);
                    if let Err(error) = session_result {
                        let code = session_error_code(&error);
                        output.send(Output::Event(SessionEvent::SessionError { session_id: id, code })).await.map_err(|_| Error::Transport)?;
                    }
                    output.send(Output::Event(SessionEvent::SessionClosed { session_id: id })).await.map_err(|_| Error::Transport)?;
                    finished_sessions.insert(id);
                }
                message = outgoing.recv() => {
                    let output_message = message.ok_or(Error::Transport)?;
                    let message = match output_message {
                        Output::Event(event) => {
                            if let SessionEvent::ResponseEnd { session_id: id, .. } = &event {
                            let conversation_id = sessions.get(id).map_or(*id, |session| session.conversation_id);
                            waiting(progress.as_ref(), conversation_id, WaitingFor::API).await;
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
                        Message::Ping(data) => {
                            writer.send(Message::Pong(data)).await.map_err(|_| Error::Transport)?;
                            continue;
                        }
                        Message::Pong(_) => continue,
                        Message::Close(_) => return Err(Error::Transport),
                        Message::Binary(bytes) => Input::Audio(Audio::decode(&bytes)?),
                        Message::Text(text) => {
                            if text.len() > 16 * 1024 * 1024 { return Err(Error::Protocol); }
                            let message: ServerMessage = serde_json::from_str(&text).map_err(|_| Error::Protocol)?;
                            if text.len() > MAX_CONTROL_BYTES
                                && !matches!(message, ServerMessage::Endpoint(EndpointEvent::VoiceResult { .. }))
                            {
                                return Err(Error::Protocol);
                            }
                            match message {
                                ServerMessage::Endpoint(event) => {
                                    event.validate()?;
                                    match event {
                                        EndpointEvent::VoiceReady { version: ProtocolVersion::V2 } => continue,
                                        EndpointEvent::VoiceReady { .. } => return Err(Error::Protocol),
                                        EndpointEvent::VoiceResult { data } => return Ok(data),
                                        EndpointEvent::VoiceError { code, .. } => return Err(Error::Remote(code)),
                                    }
                                }
                                ServerMessage::Progress(message) => {
                                    forward_progress(progress.as_ref(), message).await;
                                    continue;
                                }
                                ServerMessage::Command(command) => {
                                    command.validate()?;
                                    Input::Control(command)
                                }
                            }
                        }
                        Message::Frame(_) => return Err(Error::Protocol),
                    };

                    let id = match &input {
                        Input::Audio(audio) => audio.header.session_id,
                        Input::Control(command) => command.session_id(),
                    };
                    if overflowed_sessions.contains(&id) {
                        match input {
                            Input::Control(Command::SessionClose { .. }) => {
                                overflowed_sessions.remove(&id);
                                finished_sessions.remove(&id);
                                if let Some(session) = sessions.remove(&id) {
                                    let _ = session.stop.send(true);
                                }
                            }
                            Input::Control(Command::SessionOpen { .. }) => return Err(Error::Protocol),
                            Input::Control(Command::ExchangeStart { .. } | Command::ExchangeEnd { .. })
                            | Input::Audio(_) => {}
                        }
                        continue;
                    }
                    if let Input::Control(Command::SessionOpen {
                        conversation_id,
                        participants,
                        max_exchanges,
                        timeout_ms,
                        ..
                    }) = input {
                        if sessions.contains_key(&id) || sessions.len() >= 128 {
                            return Err(Error::Protocol);
                        }
                        let spec = MultipartySessionSpec {
                            session_id: id,
                            conversation_id,
                            participants,
                            max_exchanges,
                        };
                        let (input_tx, input_rx) = mpsc::channel(QUEUE_FRAMES + 2);
                        let (stop_tx, stop_rx) = watch::channel(false);
                        sessions.insert(id, SessionHandle { input: input_tx, stop: stop_tx, conversation_id });
                        let provider = provider.clone();
                        let output = output.clone();
                        tasks.spawn(async move {
                            let execution = async {
                                let session = tokio::time::timeout(
                                    Duration::from_millis(timeout_ms.min(3_300_000)),
                                    provider.connect_multiparty(spec),
                                )
                                .await
                                .map_err(|_| Error::Timeout)??;
                                session.run(input_rx, output, stop_rx).await
                            };
                            (id, execution.await)
                        });
                        continue;
                    }

                    if let Input::Control(Command::SessionClose { .. }) = input {
                        if finished_sessions.remove(&id) { continue; }
                        let Some(session) = sessions.remove(&id) else { return Err(Error::Protocol); };
                        let _ = session.stop.send(true);
                        continue;
                    }
                    if let Input::Control(Command::ExchangeStart { .. }) = &input {
                        let Some(session) = sessions.get(&id) else { return Err(Error::Protocol); };
                        waiting(progress.as_ref(), session.conversation_id, WaitingFor::Provider).await;
                    }
                    let Some(session) = sessions.get(&id) else { return Err(Error::Protocol); };
                    if session.input.try_send(input).is_err() {
                        let _ = session.stop.send(true);
                        overflowed_sessions.insert(id);
                        send_session_error(&mut writer, id, SessionErrorCode::Overflow).await?;
                    }
                }
            }
        }
    }
    .await;

    for session in sessions.values() {
        let _ = session.stop.send(true);
    }
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
    if let Some(progress) = progress {
        let _ = progress
            .send(MultiPartyProgressIndicatorMessage::EvaluationComplete)
            .await;
    }
    result
}

async fn waiting(
    progress: Option<&mpsc::Sender<MultiPartyProgressIndicatorMessage>>,
    conversation_id: i32,
    waiting_for: WaitingFor,
) {
    if let Some(progress) = progress {
        let _ = progress
            .send(MultiPartyProgressIndicatorMessage::WaitingFor {
                conversation_id,
                waiting_for,
            })
            .await;
    }
}

async fn forward_progress(
    progress: Option<&mpsc::Sender<MultiPartyProgressIndicatorMessage>>,
    message: MultiPartyProgressIndicatorMessage,
) {
    tracing::info!(event = ?message, "Multi-party voice progress");
    if let Some(progress) = progress {
        let _ = progress.send(message).await;
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

async fn send_session_error<S>(
    writer: &mut S,
    session_id: i32,
    code: SessionErrorCode,
) -> Result<()>
where
    S: Sink<Message> + Unpin,
{
    writer
        .send(Message::Text(
            serde_json::to_string(&SessionEvent::SessionError { session_id, code })
                .map_err(|_| Error::Protocol)?
                .into(),
        ))
        .await
        .map_err(|_| Error::Transport)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::{
        common_protocol::VoiceTransport,
        multiparty_protocol::{
            AudioHeader, ExchangeInput, GeminiVoice, LogicalParticipant, MultipartyCapabilities,
        },
        session::{MultipartyOutput, MultipartyVoiceSession, VoiceSession, VoiceSessionProvider},
        target::TargetMetadata,
    };
    use async_trait::async_trait;
    use serde_json::{Value, json};
    use std::sync::Arc;

    struct FakeProvider;
    struct FakeSession {
        id: i32,
    }

    #[async_trait]
    impl VoiceSessionProvider for FakeProvider {
        async fn connect(
            &self,
            _session_id: i32,
            _max_turns: u32,
        ) -> Result<Box<dyn VoiceSession>> {
            Err(Error::Configuration("v1 unsupported by fake"))
        }

        async fn connect_multiparty(
            &self,
            spec: MultipartySessionSpec,
        ) -> Result<Box<dyn MultipartyVoiceSession>> {
            assert_eq!(spec.participants.len(), 2);
            if spec.session_id == 9 {
                return Err(Error::Authentication);
            }
            Ok(Box::new(FakeSession {
                id: spec.session_id,
            }))
        }
    }

    #[async_trait]
    impl MultipartyVoiceSession for FakeSession {
        async fn run(
            self: Box<Self>,
            mut input: mpsc::Receiver<Input>,
            output: MultipartyOutput,
            mut stop: watch::Receiver<bool>,
        ) -> Result<()> {
            output
                .send(Output::Event(SessionEvent::SessionReady {
                    session_id: self.id,
                    capabilities: MultipartyCapabilities::REQUIRED,
                    external_session_id: None,
                    livekit_participant_identity: None,
                    livekit_track_id: None,
                }))
                .await
                .map_err(|_| Error::Transport)?;
            let mut exchange = 0;
            loop {
                tokio::select! {
                    biased;
                    _ = stop.changed() => {
                        return Ok(());
                    }
                    message = input.recv() => match message.ok_or(Error::Transport)? {
                        Input::Control(Command::ExchangeStart { exchange_id, inputs, .. }) => {
                            assert_eq!(inputs[0].speaker_id, "alice");
                            exchange = exchange_id;
                            output.send(Output::Event(SessionEvent::ExchangeReady { session_id: self.id, exchange_id: exchange })).await.map_err(|_| Error::Transport)?;
                        }
                        Input::Audio(audio) => {
                            assert_eq!(audio.header.exchange_id, exchange);
                            output.send(Output::Event(SessionEvent::AudioConsumed { session_id: self.id, exchange_id: exchange })).await.map_err(|_| Error::Transport)?;
                        }
                        Input::Control(Command::ExchangeEnd { exchange_id, .. }) => {
                            output.send(Output::Event(SessionEvent::PlaybackComplete { session_id: self.id, exchange_id, elapsed_ms: 1.0 })).await.map_err(|_| Error::Transport)?;
                            output.send(Output::Event(SessionEvent::ResponseStart { session_id: self.id, exchange_id, elapsed_ms: 2.0 })).await.map_err(|_| Error::Transport)?;
                            let audio = Audio {
                                header: AudioHeader { session_id:self.id, exchange_id, sequence:0, sample_offset:0 },
                                pcm: vec![2, 0, 3, 0],
                            };
                            output.send(Output::Audio(audio)).await.map_err(|_| Error::Transport)?;
                            output.send(Output::Event(SessionEvent::ResponseEnd { session_id: self.id, exchange_id, samples: 2, transcript: Some("comparison".into()), elapsed_ms: 3.0 })).await.map_err(|_| Error::Transport)?;
                        }
                        Input::Control(Command::SessionOpen { .. } | Command::SessionClose { .. }) => return Err(Error::Protocol),
                    }
                }
            }
        }
    }

    async fn receive_json(
        socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    ) -> Value {
        let message = socket.next().await.unwrap().unwrap();
        serde_json::from_str(message.to_text().unwrap()).unwrap()
    }

    fn session_open(id: i32) -> Value {
        json!({
            "type":"session_open",
            "session_id":id,
            "conversation_id":id,
            "delivery_mode":"single_virtual_participant",
            "participants":[
                {"id":"alice","display_name":"Alice","voice":"Aoede"},
                {"id":"bob","display_name":"Bob","voice":"Charon"}
            ],
            "max_exchanges":2,
            "timeout_ms":1000
        })
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn multiplexes_sessions_and_preserves_voice_result_metadata() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let request = receive_json(&mut socket).await;
            assert_eq!(request["version"], "2");
            assert_eq!(request["type"], "multiparty_voice_request");
            assert_eq!(request["target"]["transport"], "livekit");
            socket
                .send(Message::Text(
                    json!({"type":"voice_ready","version":"2"})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            socket.send(Message::Text(json!({"type":"multiparty_evaluation_start","data":{"conversation_ids":[1,2],"max_turns":2}}).to_string().into())).await.unwrap();
            for id in [1, 2] {
                socket
                    .send(Message::Text(session_open(id).to_string().into()))
                    .await
                    .unwrap();
            }
            let mut ready = Vec::new();
            for _ in 0..2 {
                let event = receive_json(&mut socket).await;
                assert_eq!(event["type"], "session_ready");
                ready.push(event["session_id"].as_i64().unwrap());
            }
            ready.sort_unstable();
            assert_eq!(ready, [1, 2]);

            for id in [1, 2] {
                socket.send(Message::Text(json!({
                    "type":"exchange_start","session_id":id,"exchange_id":1,"timeout_ms":1000,
                    "inputs":[{"utterance_id":1,"speaker_id":"alice","display_name":"Alice","voice":"Aoede","text":"hello"}]
                }).to_string().into())).await.unwrap();
            }
            for _ in 0..2 {
                assert_eq!(receive_json(&mut socket).await["type"], "exchange_ready");
            }
            for id in [1, 2] {
                let audio = Audio {
                    header: AudioHeader {
                        session_id: id,
                        exchange_id: 1,
                        sequence: 0,
                        sample_offset: 0,
                    },
                    pcm: vec![1, 0, 1, 0],
                };
                socket
                    .send(Message::Binary(audio.encode().unwrap().into()))
                    .await
                    .unwrap();
            }
            for _ in 0..2 {
                assert_eq!(receive_json(&mut socket).await["type"], "audio_consumed");
            }
            for id in [1, 2] {
                socket
                    .send(Message::Text(
                        json!({"type":"exchange_end","session_id":id,"exchange_id":1})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }
            let mut responses = HashMap::new();
            while responses.len() < 2 {
                let message = socket.next().await.unwrap().unwrap();
                if message.is_binary() {
                    let audio = Audio::decode(&message.into_data()).unwrap();
                    assert_eq!(audio.header.exchange_id, 1);
                    continue;
                }
                let event: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
                if event["type"] == "response_end" {
                    responses.insert(
                        event["session_id"].as_i64().unwrap(),
                        event["transcript"].clone(),
                    );
                }
            }
            assert_eq!(responses.len(), 2);
            for id in [1, 2] {
                socket
                    .send(Message::Text(
                        json!({"type":"session_close","session_id":id})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }
            for _ in 0..2 {
                assert_eq!(receive_json(&mut socket).await["type"], "session_closed");
            }
            socket.send(Message::Text(json!({
                "type":"voice_result",
                "data":{
                    "evaluation_id":42,"total_passed":2,"total_failed":0,"total_errors":0,"results":[],
                    "target":{"label":"fake target","transport":"livekit","cli_version":env!("CARGO_PKG_VERSION")},
                    "voice_test_case_ids":{"1":17,"2":18},
                    "voice_metadata":{"1":[{
                        "exchange_id":1,
                        "inputs":[{"utterance_id":1,"speaker_id":"alice","display_name":"Alice","voice":"Aoede","source_text":"hello","tagged_text":"[warmly] hello","duration_seconds":0.1}],
                        "customer_response":{"transcription":"hi","comparison_transcript":"comparison","duration_seconds":0.2},
                        "transport_metadata":{"external_session_id":"external","livekit_participant_identity":"virtual","livekit_track_id":"track"},
                        "stage_timings":{"total":0.3}
                    }],"2":[]}
                }
            }).to_string().into())).await.unwrap();
            while let Some(Ok(message)) = socket.next().await {
                if message.is_close() {
                    break;
                }
            }
        });
        let (websocket, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
            .await
            .unwrap();
        let target = PreparedTarget {
            provider: Arc::new(FakeProvider),
            metadata: TargetMetadata {
                label: "fake target".into(),
                transport: VoiceTransport::Livekit,
            },
        };
        let request = MultiPartyEvaluationRequest::Standard(
            crate::protocol_types::multi_party::MultiPartyRequest {
                test_case_groups: vec!["pack".into()],
                threshold: 0.5,
                max_turns: 2,
            },
        );
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            run_multiparty(websocket, target, request, None),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result.result.evaluation_id, 42);
        assert_eq!(result.voice_test_case_ids["1"], 17);
        assert_eq!(
            result.voice_metadata["1"][0].inputs[0].voice,
            GeminiVoice::Aoede
        );
        server.await.unwrap();
    }

    #[test]
    fn protocol_manifest_types_are_serializable() {
        let participant = LogicalParticipant {
            id: "alice".into(),
            display_name: "Alice".into(),
            voice: GeminiVoice::Aoede,
        };
        let input = ExchangeInput {
            utterance_id: 1,
            speaker_id: participant.id.clone(),
            display_name: participant.display_name.clone(),
            voice: participant.voice,
            text: "hello".into(),
        };
        assert_eq!(serde_json::to_value(input).unwrap()["voice"], "Aoede");
    }

    #[tokio::test]
    async fn failed_session_reports_error_before_closed_and_accepts_close() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            receive_json(&mut socket).await;
            socket
                .send(Message::Text(
                    json!({"type":"voice_ready","version":"2"})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            socket
                .send(Message::Text(session_open(9).to_string().into()))
                .await
                .unwrap();
            let error = receive_json(&mut socket).await;
            assert_eq!(error["type"], "session_error");
            assert_eq!(error["code"], "authentication");
            assert_eq!(receive_json(&mut socket).await["type"], "session_closed");
            socket
                .send(Message::Text(
                    json!({"type":"session_close","session_id":9})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            socket
                .send(Message::Text(
                    json!({
                        "type":"voice_result",
                        "data":{
                            "evaluation_id":1,"total_passed":0,"total_failed":0,"total_errors":1,"results":[],
                            "target":{"label":"fake target","transport":"livekit","cli_version":env!("CARGO_PKG_VERSION")},
                            "voice_test_case_ids":{},"voice_metadata":{}
                        }
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
        });
        let (websocket, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
            .await
            .unwrap();
        let target = PreparedTarget {
            provider: Arc::new(FakeProvider),
            metadata: TargetMetadata {
                label: "fake target".into(),
                transport: VoiceTransport::Livekit,
            },
        };
        let request = MultiPartyEvaluationRequest::Standard(
            crate::protocol_types::multi_party::MultiPartyRequest {
                test_case_groups: vec!["pack".into()],
                threshold: 0.5,
                max_turns: 2,
            },
        );
        let result = run_multiparty(websocket, target, request, None)
            .await
            .unwrap();
        assert_eq!(result.result.total_errors, 1);
        server.await.unwrap();
    }
}
