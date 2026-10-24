use super::EvaluationError;
use crate::protocol_types::common::{CompletionErrorEnvelope, CompletionResponseEnvelope};
use crate::protocol_types::{self};
use crate::response_provider::ResponseProvider;
use crate::websockets::WebSocketConnection;

use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{self, Message};

pub(super) enum ParsedIncoming<FinalResponse, OptionalMessage> {
    CompletionRequest(protocol_types::CompletionRequest),
    FinalResponse(FinalResponse),
    OptionalMessage(OptionalMessage),
}

#[derive(Debug, PartialEq)]
enum TransportEvent {
    Text(String),
    Ping(Vec<u8>),
    Close(Option<CloseFrame>),
    ReadError(String),
    Ignore,
}

#[derive(Debug, PartialEq)]
struct CloseDirective {
    code: CloseCode,
    reason: String,
}

#[derive(Debug)]
enum OutboundProtocolMessage {
    CompletionResponse(protocol_types::CompletionResponse),
    CompletionError(protocol_types::CompletionError),
}

#[derive(Debug)]
enum OutboundEvent {
    Protocol(OutboundProtocolMessage),
    Pong(Vec<u8>),
    Close(CloseDirective),
}

struct CompletionTaskOutput<ProgressMessage> {
    progress: Vec<ProgressMessage>,
    outbound: OutboundProtocolMessage,
}

pub(super) trait EvaluationMode: Clone + Send + Sync + 'static {
    type Request: Send + 'static;
    type FinalResponse: Send + 'static;
    type OptionalMessage: Send + 'static;
    type ProgressMessage: Send + 'static;

    fn serialize_request(&self, request: &Self::Request) -> Result<String, serde_json::Error>;

    fn parse_text(
        &self,
        text: &str,
    ) -> Result<ParsedIncoming<Self::FinalResponse, Self::OptionalMessage>, serde_json::Error>;

    fn progress_on_completion_start(
        &self,
        request: &protocol_types::CompletionRequest,
    ) -> Vec<Self::ProgressMessage>;

    fn progress_on_completion_success(
        &self,
        request: &protocol_types::CompletionRequest,
    ) -> Vec<Self::ProgressMessage>;

    fn progress_from_optional(&self, message: Self::OptionalMessage) -> Vec<Self::ProgressMessage>;

    fn evaluation_complete_progress(&self) -> Option<Self::ProgressMessage>;

    fn expected_response_name(&self) -> &'static str;
}

fn classify_transport_message(result: Result<Message, tungstenite::Error>) -> TransportEvent {
    match result {
        Ok(Message::Text(text)) => TransportEvent::Text(text.to_string()),
        Ok(Message::Ping(payload)) => TransportEvent::Ping(payload.to_vec()),
        Ok(Message::Close(frame)) => TransportEvent::Close(frame),
        Ok(other) => {
            tracing::warn!("Received unexpected websocket message: {other:?}");
            TransportEvent::Ignore
        }
        Err(err) => TransportEvent::ReadError(format!("Couldn't receive message '{err}'")),
    }
}

fn serialize_outbound_protocol_message(
    message: OutboundProtocolMessage,
) -> Result<String, serde_json::Error> {
    match message {
        OutboundProtocolMessage::CompletionResponse(response) => {
            serde_json::to_string(&CompletionResponseEnvelope::from(response))
        }
        OutboundProtocolMessage::CompletionError(error) => {
            serde_json::to_string(&CompletionErrorEnvelope::from(error))
        }
    }
}

fn websocket_message_from_outbound(event: OutboundEvent) -> Result<Message, serde_json::Error> {
    match event {
        OutboundEvent::Protocol(message) => Ok(Message::Text(
            serialize_outbound_protocol_message(message)?.into(),
        )),
        OutboundEvent::Pong(payload) => Ok(Message::Pong(payload.into())),
        OutboundEvent::Close(close) => Ok(Message::Close(Some(CloseFrame {
            code: close.code,
            reason: close.reason.into(),
        }))),
    }
}

async fn execute_completion_request<M: EvaluationMode>(
    request: protocol_types::CompletionRequest,
    provider: Arc<dyn ResponseProvider>,
    mode: M,
) -> CompletionTaskOutput<M::ProgressMessage> {
    use tracing::Instrument;
    let span = tracing::warn_span!("target_completion", request_id = %request.request_id);
    match provider
        .generate_response(&request.messages)
        .instrument(span)
        .await
    {
        Ok(completion) => CompletionTaskOutput {
            progress: mode.progress_on_completion_success(&request),
            outbound: OutboundProtocolMessage::CompletionResponse(
                protocol_types::CompletionResponse {
                    request_id: request.request_id,
                    model_response: completion.message.content,
                    model_id: completion.model_id,
                    tokens_used: completion.tokens_used,
                    finish_reason: completion.finish_reason,
                    provider_response_id: completion.provider_response_id,
                },
            ),
        },
        Err(err) => {
            tracing::error!("Error generating response: {err}");
            CompletionTaskOutput {
                progress: Vec::new(),
                outbound: OutboundProtocolMessage::CompletionError(
                    protocol_types::CompletionError {
                        request_id: request.request_id,
                        error_reason: (&err).into(),
                    },
                ),
            }
        }
    }
}

pub(super) async fn run_evaluation<M>(
    websocket_connection: WebSocketConnection,
    provider: Arc<dyn ResponseProvider>,
    request: M::Request,
    progress_indicator: Option<mpsc::Sender<M::ProgressMessage>>,
    mode: M,
) -> Result<M::FinalResponse, EvaluationError>
where
    M: EvaluationMode,
{
    let (mut write, mut read) = websocket_connection.split();
    write
        .send(Message::Text(mode.serialize_request(&request)?.into()))
        .await?;

    let mut completion_tasks = JoinSet::<CompletionTaskOutput<M::ProgressMessage>>::new();
    let progress_indicator_ref = progress_indicator.as_ref();

    let session_result = async {
        loop {
            tokio::select! {
                join_result = completion_tasks.join_next(), if !completion_tasks.is_empty() => {
                    if let Some(join_result) = join_result {
                        let output = join_result?;
                        emit_progress(progress_indicator_ref, output.progress).await?;
                        send_outbound(&mut write, OutboundEvent::Protocol(output.outbound)).await?;
                    }
                }
                message = read.next() => {
                    let Some(message) = message else {
                        break Err(EvaluationError::from_close_frame_or_eof(
                            None,
                            mode.expected_response_name(),
                        ));
                    };

                    match classify_transport_message(message) {
                        TransportEvent::Text(text) => {
                            if let Some(response) = handle_text_message(
                                &mut write,
                                &mode,
                                text,
                                provider.clone(),
                                progress_indicator_ref,
                                &mut completion_tasks,
                            )
                            .await?
                            {
                                break Ok(response);
                            }
                        }
                        TransportEvent::Ping(payload) => {
                            tracing::debug!("Received ping from server, sending pong");
                            send_outbound(&mut write, OutboundEvent::Pong(payload)).await?;
                        }
                        TransportEvent::Close(frame) => {
                            if let Some(frame) = &frame {
                                tracing::debug!(
                                    "Received close message from server with code {} and reason '{}'",
                                    frame.code,
                                    frame.reason
                                );
                            } else {
                                tracing::debug!("Received close message from server without close frame");
                            }

                            break Err(EvaluationError::from_close_frame_or_eof(
                                frame.as_ref(),
                                mode.expected_response_name(),
                            ));
                        }
                        TransportEvent::ReadError(reason) => {
                            tracing::error!("{reason}");
                            send_outbound(
                                &mut write,
                                OutboundEvent::Close(CloseDirective {
                                    code: CloseCode::Error,
                                    reason: reason.clone(),
                                }),
                            )
                            .await?;
                            break Err(EvaluationError::WebSocketClosed(reason));
                        }
                        TransportEvent::Ignore => {}
                    }
                }
            }
        }
    }
    .await;

    drain_completion_tasks(&mut completion_tasks).await?;
    session_result
}

async fn handle_text_message<M>(
    write: &mut SplitSink<WebSocketConnection, Message>,
    mode: &M,
    text: String,
    provider: Arc<dyn ResponseProvider>,
    progress_indicator: Option<&mpsc::Sender<M::ProgressMessage>>,
    completion_tasks: &mut JoinSet<CompletionTaskOutput<M::ProgressMessage>>,
) -> Result<Option<M::FinalResponse>, EvaluationError>
where
    M: EvaluationMode,
{
    match mode.parse_text(&text) {
        Ok(ParsedIncoming::CompletionRequest(request)) => {
            emit_progress(
                progress_indicator,
                mode.progress_on_completion_start(&request),
            )
            .await?;

            completion_tasks.spawn(execute_completion_request(request, provider, mode.clone()));
            Ok(None)
        }
        Ok(ParsedIncoming::FinalResponse(response)) => {
            if let Some(progress) = mode.evaluation_complete_progress() {
                emit_progress(progress_indicator, vec![progress]).await?;
            }

            Ok(Some(response))
        }
        Ok(ParsedIncoming::OptionalMessage(optional)) => {
            emit_progress(progress_indicator, mode.progress_from_optional(optional)).await?;
            Ok(None)
        }
        Err(err) => {
            let reason = format!("Failed to parse incoming message: {err}");
            tracing::error!("{reason}");
            send_outbound(
                write,
                OutboundEvent::Close(CloseDirective {
                    code: CloseCode::Protocol,
                    reason: reason.clone(),
                }),
            )
            .await?;
            Err(EvaluationError::WebSocketClosed(reason))
        }
    }
}

async fn emit_progress<ProgressMessage>(
    progress_indicator: Option<&mpsc::Sender<ProgressMessage>>,
    progress_messages: Vec<ProgressMessage>,
) -> Result<(), EvaluationError>
where
    ProgressMessage: Send,
{
    if let Some(progress_indicator) = progress_indicator {
        for progress_message in progress_messages {
            progress_indicator.send(progress_message).await?;
        }
    }

    Ok(())
}

async fn send_outbound(
    write: &mut SplitSink<WebSocketConnection, Message>,
    event: OutboundEvent,
) -> Result<(), EvaluationError> {
    write.send(websocket_message_from_outbound(event)?).await?;
    Ok(())
}

async fn drain_completion_tasks<ProgressMessage: 'static>(
    completion_tasks: &mut JoinSet<CompletionTaskOutput<ProgressMessage>>,
) -> Result<(), EvaluationError> {
    completion_tasks.abort_all();

    while let Some(join_result) = completion_tasks.join_next().await {
        match join_result {
            Ok(_) => {}
            Err(err) if err.is_cancelled() => {}
            Err(err) => return Err(err.into()),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        CloseDirective, CompletionTaskOutput, EvaluationMode, OutboundEvent,
        OutboundProtocolMessage, ParsedIncoming, TransportEvent, classify_transport_message,
        execute_completion_request, serialize_outbound_protocol_message,
        websocket_message_from_outbound,
    };
    use crate::protocol_types::{self, Role};
    use crate::response_provider::{ProviderCompletion, ProviderError, ResponseProvider};

    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::Arc;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
    use tokio_tungstenite::tungstenite::{self, Message};

    #[derive(Clone)]
    struct TestMode;

    #[derive(Debug, PartialEq)]
    enum TestOptionalMessage {
        Progress(String),
    }

    #[derive(Debug, PartialEq)]
    struct TestFinalResponse {
        status: String,
    }

    impl EvaluationMode for TestMode {
        type Request = ();
        type FinalResponse = TestFinalResponse;
        type OptionalMessage = TestOptionalMessage;
        type ProgressMessage = String;

        fn serialize_request(&self, _request: &Self::Request) -> Result<String, serde_json::Error> {
            Ok("{\"type\":\"request\"}".to_string())
        }

        fn parse_text(
            &self,
            text: &str,
        ) -> Result<ParsedIncoming<Self::FinalResponse, Self::OptionalMessage>, serde_json::Error>
        {
            let value: serde_json::Value = serde_json::from_str(text)?;
            match value.get("kind").and_then(serde_json::Value::as_str) {
                Some("completion") => Ok(ParsedIncoming::CompletionRequest(
                    protocol_types::CompletionRequest {
                        request_id: "req-1".to_string(),
                        conversation_id: 7,
                        messages: vec![protocol_types::Message {
                            role: Role::User,
                            content: "hello".to_string(),
                        }],
                    },
                )),
                Some("optional") => Ok(ParsedIncoming::OptionalMessage(
                    TestOptionalMessage::Progress("optional".to_string()),
                )),
                Some("final") => Ok(ParsedIncoming::FinalResponse(TestFinalResponse {
                    status: "done".to_string(),
                })),
                _ => Err(serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "invalid payload",
                ))),
            }
        }

        fn progress_on_completion_start(
            &self,
            _request: &protocol_types::CompletionRequest,
        ) -> Vec<Self::ProgressMessage> {
            vec!["provider".to_string()]
        }

        fn progress_on_completion_success(
            &self,
            _request: &protocol_types::CompletionRequest,
        ) -> Vec<Self::ProgressMessage> {
            vec!["api".to_string()]
        }

        fn progress_from_optional(
            &self,
            message: Self::OptionalMessage,
        ) -> Vec<Self::ProgressMessage> {
            match message {
                TestOptionalMessage::Progress(label) => vec![label],
            }
        }

        fn evaluation_complete_progress(&self) -> Option<Self::ProgressMessage> {
            Some("complete".to_string())
        }

        fn expected_response_name(&self) -> &'static str {
            "TestFinalResponse"
        }
    }

    struct SuccessProvider;

    #[async_trait]
    impl ResponseProvider for SuccessProvider {
        async fn generate_response(
            &self,
            _conversation_history: &[protocol_types::Message],
        ) -> Result<ProviderCompletion, ProviderError> {
            Ok(ProviderCompletion {
                message: protocol_types::Message {
                    role: Role::Assistant,
                    content: "safe reply".to_string(),
                },
                model_id: Some("model-version".to_string()),
                tokens_used: Some(42),
                finish_reason: Some("stop".to_string()),
                provider_response_id: Some("provider-response".to_string()),
            })
        }
    }

    struct FailingProvider;

    #[async_trait]
    impl ResponseProvider for FailingProvider {
        async fn generate_response(
            &self,
            _conversation_history: &[protocol_types::Message],
        ) -> Result<ProviderCompletion, ProviderError> {
            Err(ProviderError::Network("timeout".to_string()))
        }
    }

    #[test]
    fn classifies_transport_messages() {
        assert_eq!(
            classify_transport_message(Ok(Message::Text("{\"ok\":true}".into()))),
            TransportEvent::Text("{\"ok\":true}".to_string())
        );
        assert_eq!(
            classify_transport_message(Ok(Message::Ping(vec![1, 2, 3].into()))),
            TransportEvent::Ping(vec![1, 2, 3])
        );
        assert_eq!(
            classify_transport_message(Err(tungstenite::Error::ConnectionClosed)),
            TransportEvent::ReadError(
                "Couldn't receive message 'Connection closed normally'".to_string()
            )
        );
    }

    #[test]
    fn serializes_outbound_protocol_messages() {
        let text = serialize_outbound_protocol_message(
            OutboundProtocolMessage::CompletionResponse(protocol_types::CompletionResponse {
                request_id: "req-1".to_string(),
                model_response: "safe".to_string(),
                model_id: Some("model-version".to_string()),
                tokens_used: Some(42),
                finish_reason: Some("stop".to_string()),
                provider_response_id: Some("provider-response".to_string()),
            }),
        )
        .expect("protocol message should serialize");

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).expect("json should parse"),
            json!({
                "type": "completion_response",
                "data": {
                    "request_id": "req-1",
                    "model_response": "safe",
                    "model_id": "model-version",
                    "tokens_used": 42,
                    "finish_reason": "stop",
                    "provider_response_id": "provider-response"
                }
            })
        );
    }

    #[test]
    fn converts_outbound_events_to_websocket_messages() {
        let message = websocket_message_from_outbound(OutboundEvent::Close(CloseDirective {
            code: CloseCode::Protocol,
            reason: "bad payload".to_string(),
        }))
        .expect("close event should convert");

        match message {
            Message::Close(Some(frame)) => {
                assert_eq!(frame.code, CloseCode::Protocol);
                assert_eq!(frame.reason, "bad payload");
            }
            other => panic!("expected close message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn completion_success_yields_api_progress_and_response() {
        let output: CompletionTaskOutput<String> = execute_completion_request(
            protocol_types::CompletionRequest {
                request_id: "req-1".to_string(),
                conversation_id: 9,
                messages: vec![protocol_types::Message {
                    role: Role::User,
                    content: "hello".to_string(),
                }],
            },
            Arc::new(SuccessProvider),
            TestMode,
        )
        .await;

        assert_eq!(output.progress, vec!["api".to_string()]);
        assert!(matches!(
            output.outbound,
            OutboundProtocolMessage::CompletionResponse(protocol_types::CompletionResponse {
                request_id,
                model_response,
                model_id,
                tokens_used,
                finish_reason,
                provider_response_id,
            }) if request_id == "req-1"
                && model_response == "safe reply"
                && model_id.as_deref() == Some("model-version")
                && tokens_used == Some(42)
                && finish_reason.as_deref() == Some("stop")
                && provider_response_id.as_deref() == Some("provider-response")
        ));
    }

    #[tokio::test]
    async fn completion_failure_yields_completion_error_without_progress() {
        let output: CompletionTaskOutput<String> = execute_completion_request(
            protocol_types::CompletionRequest {
                request_id: "req-2".to_string(),
                conversation_id: 11,
                messages: vec![protocol_types::Message {
                    role: Role::User,
                    content: "hello".to_string(),
                }],
            },
            Arc::new(FailingProvider),
            TestMode,
        )
        .await;

        assert_eq!(output.progress, [] as [std::string::String; 0]);
        assert!(matches!(
            output.outbound,
            OutboundProtocolMessage::CompletionError(protocol_types::CompletionError {
                request_id,
                ..
            }) if request_id == "req-2"
        ));
    }
    #[tokio::test]
    async fn reported_failure_preserves_reason_without_success_progress() {
        struct ReportedFailure;
        #[async_trait]
        impl ResponseProvider for ReportedFailure {
            async fn generate_response(
                &self,
                _history: &[protocol_types::Message],
            ) -> Result<ProviderCompletion, ProviderError> {
                Err(ProviderError::Reported {
                    reason: protocol_types::common::CompletionErrorReason::ModelTimeout,
                    message: "Rate auth key".to_string(),
                })
            }
        }
        let output = execute_completion_request(
            protocol_types::CompletionRequest {
                request_id: "reported-error".to_string(),
                conversation_id: 1,
                messages: vec![],
            },
            Arc::new(ReportedFailure),
            TestMode,
        )
        .await;
        assert_eq!(output.progress, [] as [String; 0]);
        let text = serialize_outbound_protocol_message(output.outbound).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).unwrap(),
            json!({
                "type": "completion_error",
                "data": {"request_id": "reported-error", "error_reason": "model_timeout"},
            })
        );
    }
    #[tokio::test]
    async fn retry_success_emits_one_response_and_one_progress_update() {
        use crate::response_provider::{
            HttpResponseProvider,
            test_support::{StubApi, serve_responses},
        };
        let (url, server) =
            serve_responses(vec![(429, None, "busy".into()), (200, None, "{}".into())]).await;
        let api = StubApi::new(
            url,
            vec![
                Err(ProviderError::Reported {
                    reason: protocol_types::common::CompletionErrorReason::RateLimited,
                    message: "busy".into(),
                }),
                SuccessProvider.generate_response(&[]).await,
            ],
        );
        let builds = api.builds.clone();
        let provider = Arc::new(
            HttpResponseProvider::new(Box::new(api), &reqwest::header::HeaderMap::new()).unwrap(),
        );
        let output = execute_completion_request(
            protocol_types::CompletionRequest {
                request_id: "retry-id".into(),
                conversation_id: 1,
                messages: vec![],
            },
            provider.clone(),
            TestMode,
        )
        .await;
        assert_eq!(builds.lock().unwrap().len(), 2);
        assert_eq!(server.await.unwrap().len(), 2);
        assert_eq!(output.progress, vec!["api"]);
        assert!(
            matches!(output.outbound, OutboundProtocolMessage::CompletionResponse(response)
            if response.request_id == "retry-id" && response.tokens_used == Some(42))
        );
    }

    #[tokio::test]
    async fn websocket_responds_during_retry_and_shutdown_cancels_wait() {
        use crate::response_provider::{
            HttpResponseProvider,
            test_support::{StubApi, serve_responses},
        };
        use futures_util::{SinkExt, StreamExt};
        let (url, http_server) =
            serve_responses(vec![(429, Some("3600".into()), "busy".into())]).await;
        let api = StubApi::new(
            url,
            vec![Err(ProviderError::RateLimited {
                message: "busy".into(),
                retry_after: None,
            })],
        );
        let called = api.parsed.clone();
        let builds = api.builds.clone();
        let provider = Arc::new(
            HttpResponseProvider::new(Box::new(api), &reqwest::header::HeaderMap::new()).unwrap(),
        );
        let (websocket, server) = crate::evaluations::test_support::spawn_websocket_server(
            move |mut socket| async move {
                socket.next().await.unwrap().unwrap();
                socket
                    .send(Message::Text(r#"{"kind":"completion"}"#.into()))
                    .await
                    .unwrap();
                called.notified().await;
                socket.send(Message::Ping(vec![1, 2].into())).await.unwrap();
                assert!(matches!(
                    socket.next().await.unwrap().unwrap(),
                    Message::Pong(_)
                ));
                socket
                    .send(Message::Text(r#"{"kind":"final"}"#.into()))
                    .await
                    .unwrap();
            },
        )
        .await;
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            super::run_evaluation(websocket, provider.clone(), (), None, TestMode),
        )
        .await
        .expect("evaluation must not wait for retry on shutdown")
        .unwrap();
        assert_eq!(response.status, "done");
        server.await.unwrap();
        assert_eq!(builds.lock().unwrap().len(), 1);
        assert_eq!(http_server.await.unwrap().len(), 1);
    }
}
