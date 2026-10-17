//! Exercise the shared runner without a native transport or external service.
use super::{
    Result,
    protocol::{Audio, AudioHeader, Input},
    runner,
    session::{Output, VoiceSession, VoiceSessionProvider, emit},
    target::{PreparedTarget, TargetMetadata},
};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

struct FakeProvider(Arc<AtomicBool>);
struct FakeSession(Arc<AtomicBool>);

#[async_trait]
impl VoiceSessionProvider for FakeProvider {
    async fn connect(&self, id: i32, max_turns: u32) -> Result<Box<dyn VoiceSession>> {
        assert_eq!(id, 7);
        assert!(matches!(max_turns, 1 | 4));
        Ok(Box::new(FakeSession(self.0.clone())))
    }
}

#[async_trait]
impl VoiceSession for FakeSession {
    async fn run(
        self: Box<Self>,
        mut input: mpsc::Receiver<Input>,
        output: Output,
        mut stop: watch::Receiver<bool>,
    ) -> Result<()> {
        emit(&output, json!({"type":"session_ready","session_id":7})).await?;
        loop {
            tokio::select! {
                _ = stop.changed() => break,
                item = input.recv() => match item {
                    Some(Input::Control(command)) => {
                        assert_eq!(command.session_id(), 7);
                        emit(&output, json!({"type":"utterance_ready","session_id":7,"stream_id":1})).await?;
                    }
                    Some(Input::Audio(audio)) => {
                        assert_eq!(audio.pcm, [1, 0, 2, 0]);
                        output.send(Message::Binary(audio.encode()?.into())).await.unwrap();
                    }
                    None => break,
                }
            }
        }
        self.0.store(true, Ordering::SeqCst);
        Ok(())
    }
}

async fn receive_json(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) -> Value {
    let message = socket.next().await.unwrap().unwrap();
    serde_json::from_str(message.to_text().unwrap()).unwrap()
}

async fn exercise(mode: u8, disconnect: bool) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let request = receive_json(&mut socket).await;
        assert_eq!(
            request["target"],
            json!({"label":"fake target","transport":"fake","cli_version":env!("CARGO_PKG_VERSION")})
        );
        assert_eq!(
            request["type"],
            if mode == 2 {
                "voice_rerun_request"
            } else {
                "voice_request"
            }
        );
        socket.send(Message::Text(json!({"type":"session_open","session_id":7,"max_turns":if mode == 0 {1} else {4},"timeout_ms":1000}).to_string().into())).await.unwrap();
        assert_eq!(receive_json(&mut socket).await["type"], "session_ready");
        socket.send(Message::Text(json!({"type":"utterance_start","session_id":7,"stream_id":1,"text":"hello","timeout_ms":1000}).to_string().into())).await.unwrap();
        assert_eq!(receive_json(&mut socket).await["type"], "utterance_ready");
        let audio = Audio {
            header: AudioHeader {
                session_id: 7,
                stream_id: 1,
                sequence: 0,
                sample_offset: 0,
            },
            pcm: vec![1, 0, 2, 0],
        };
        socket
            .send(Message::Binary(audio.encode().unwrap().into()))
            .await
            .unwrap();
        let response = socket.next().await.unwrap().unwrap();
        assert_eq!(Audio::decode(&response.into_data()).unwrap(), audio);
        if disconnect {
            socket.close(None).await.unwrap();
        } else {
            socket.send(Message::Text(json!({"type":"voice_result","data":{"evaluation_id":1,"total_passed":0,"total_failed":0,"results":[]}}).to_string().into())).await.unwrap();
            // Keep the connection alive until the runner finishes cleanup.
            while let Some(Ok(message)) = socket.next().await {
                if message.is_close() {
                    break;
                }
            }
        }
    });
    let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
        .await
        .unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let target = PreparedTarget {
        provider: Arc::new(FakeProvider(stopped.clone())),
        metadata: TargetMetadata {
            label: "fake target".into(),
            transport: "fake",
        },
    };
    let result = if mode == 0 {
        let request = serde_json::from_value(json!({"threshold":0.75,"variations":1,"maximum_iteration_layers":1,"test_case_groups":["test"]})).unwrap();
        runner::run_single(
            socket,
            target,
            crate::protocol_types::SingleTurnEvaluationRequest::Standard(request),
            None,
        )
        .await
    } else {
        let request = if mode == 1 {
            crate::protocol_types::MultiTurnEvaluationRequest::Standard(
                serde_json::from_value(
                    json!({"threshold":0.75,"max_turns":4,"test_case_groups":["test"]}),
                )
                .unwrap(),
            )
        } else {
            crate::protocol_types::MultiTurnEvaluationRequest::Rerun(
                serde_json::from_value(json!({"threshold":0.75,"max_turns":4,"evaluation_id":1}))
                    .unwrap(),
            )
        };
        runner::run(socket, target, request, None).await
    };
    assert_eq!(result.is_err(), disconnect);
    if let Ok(result) = result {
        assert_eq!(result["evaluation_id"], 1);
    }
    assert!(stopped.load(Ordering::SeqCst));
    server.await.unwrap();
}

#[tokio::test]
async fn all_voice_entrypoints_work_without_livekit() {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        for mode in 0..3 {
            exercise(mode, false).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn api_disconnect_stops_adapter_sessions() {
    tokio::time::timeout(std::time::Duration::from_secs(10), exercise(1, true))
        .await
        .unwrap();
}
