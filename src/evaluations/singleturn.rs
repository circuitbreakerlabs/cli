use super::EvaluationError;
use super::engine::{self, EvaluationMode, ParsedIncoming};
use crate::protocol_types::single_turn::{
    CategorizedSingleTurnMessage, OptionalSingleTurnMessage, SingleTurnReceivableMessage,
    SingleTurnRequest, SingleTurnRequestEnvelope, SingleTurnResponse,
};
use crate::protocol_types::{self};
use crate::response_provider::ResponseProvider;
use crate::tui::SingleTurnProgressIndicatorMessage;
use crate::tui::WaitingFor;
use crate::websockets::WebSocketConnection;

use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Clone, Copy)]
struct SingleTurnMode;

impl EvaluationMode for SingleTurnMode {
    type Request = SingleTurnRequest;
    type FinalResponse = SingleTurnResponse;
    type OptionalMessage = OptionalSingleTurnMessage;
    type ProgressMessage = SingleTurnProgressIndicatorMessage;

    fn serialize_request(&self, request: &Self::Request) -> Result<String, serde_json::Error> {
        serde_json::to_string(&SingleTurnRequestEnvelope::from(request.clone()))
    }

    fn parse_text(
        &self,
        text: &str,
    ) -> Result<ParsedIncoming<Self::FinalResponse, Self::OptionalMessage>, serde_json::Error> {
        SingleTurnReceivableMessage::try_from(text.as_bytes())
            .map(CategorizedSingleTurnMessage::from)
            .map(|message| match message {
                CategorizedSingleTurnMessage::CompletionRequest(request) => {
                    ParsedIncoming::CompletionRequest(request)
                }
                CategorizedSingleTurnMessage::SingleTurnResponse(response) => {
                    ParsedIncoming::FinalResponse(response)
                }
                CategorizedSingleTurnMessage::OptionalSingleTurnMessage(message) => {
                    ParsedIncoming::OptionalMessage(message)
                }
            })
    }

    fn progress_on_completion_start(
        &self,
        request: &protocol_types::CompletionRequest,
    ) -> Vec<Self::ProgressMessage> {
        vec![SingleTurnProgressIndicatorMessage::WaitingFor {
            conversation_id: request.conversation_id,
            waiting_for: WaitingFor::Provider,
        }]
    }

    fn progress_on_completion_success(
        &self,
        request: &protocol_types::CompletionRequest,
    ) -> Vec<Self::ProgressMessage> {
        vec![SingleTurnProgressIndicatorMessage::WaitingFor {
            conversation_id: request.conversation_id,
            waiting_for: WaitingFor::API,
        }]
    }

    fn progress_from_optional(&self, message: Self::OptionalMessage) -> Vec<Self::ProgressMessage> {
        match message {
            OptionalSingleTurnMessage::IterationStart(iteration_start) => {
                tracing::info!(
                    "Received IterationStart message: iteration_number={}, conversation_ids={:?}",
                    iteration_start.iteration_number,
                    iteration_start.conversation_ids,
                );
                vec![SingleTurnProgressIndicatorMessage::IterationStart(
                    iteration_start,
                )]
            }
            OptionalSingleTurnMessage::IterationComplete(iteration_complete) => {
                tracing::info!(
                    "Received IterationComplete message: iteration_number={}, passed_conversation_ids={:?}, failed_conversation_ids={:?}",
                    iteration_complete.iteration_number,
                    iteration_complete.passed_conversation_ids,
                    iteration_complete.failed_conversation_ids
                );
                vec![SingleTurnProgressIndicatorMessage::IterationComplete(
                    iteration_complete,
                )]
            }
            OptionalSingleTurnMessage::ConversationError(conversation_error) => {
                tracing::error!(
                    "Received ConversationError message: conversation_id={}, error_message={}",
                    conversation_error.conversation_id,
                    conversation_error.error_message
                );
                vec![SingleTurnProgressIndicatorMessage::ConversationError(
                    conversation_error,
                )]
            }
            OptionalSingleTurnMessage::ConversationComplete(conversation_complete) => {
                tracing::info!(
                    "Received ConversationComplete message: conversation_id={}, passed={}",
                    conversation_complete.conversation_id,
                    conversation_complete.passed
                );
                vec![SingleTurnProgressIndicatorMessage::ConversationComplete(
                    conversation_complete,
                )]
            }
        }
    }

    fn evaluation_complete_progress(&self) -> Option<Self::ProgressMessage> {
        Some(SingleTurnProgressIndicatorMessage::EvaluationComplete)
    }

    fn expected_response_name(&self) -> &'static str {
        "SingleTurnResponse"
    }
}

pub async fn run_evaluation(
    websocket_connection: WebSocketConnection,
    provider: Arc<dyn ResponseProvider>,
    request: SingleTurnRequest,
    progress_indicator: Option<mpsc::Sender<SingleTurnProgressIndicatorMessage>>,
) -> Result<SingleTurnResponse, EvaluationError> {
    engine::run_evaluation(
        websocket_connection,
        provider,
        request,
        progress_indicator,
        SingleTurnMode,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::{EvaluationMode, OptionalSingleTurnMessage, SingleTurnMode, run_evaluation};
    use crate::evaluations::EvaluationError;
    use crate::evaluations::test_support::{
        ControlledProvider, ProviderBehavior, gated_behavior, recv_text_json, send_json,
        spawn_websocket_server,
    };
    use crate::protocol_types::common::{ConversationComplete, ConversationError, ServerErrorCode};
    use crate::protocol_types::single_turn::{
        IterationComplete, IterationStart, SingleTurnRequest,
    };
    use crate::protocol_types::{self, Role};
    use crate::response_provider::ProviderError;
    use crate::tui::SingleTurnProgressIndicatorMessage;
    use crate::tui::WaitingFor;
    use futures_util::{SinkExt, StreamExt};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

    #[test]
    fn completion_progress_maps_to_waiting_states() {
        let request = protocol_types::CompletionRequest {
            request_id: "req-1".to_string(),
            conversation_id: 42,
            messages: vec![protocol_types::Message {
                role: Role::User,
                content: "hello".to_string(),
            }],
        };

        let start = SingleTurnMode.progress_on_completion_start(&request);
        let success = SingleTurnMode.progress_on_completion_success(&request);

        assert!(matches!(
            start.as_slice(),
            [SingleTurnProgressIndicatorMessage::WaitingFor {
                conversation_id: 42,
                waiting_for: WaitingFor::Provider
            }]
        ));
        assert!(matches!(
            success.as_slice(),
            [SingleTurnProgressIndicatorMessage::WaitingFor {
                conversation_id: 42,
                waiting_for: WaitingFor::API
            }]
        ));
    }

    #[test]
    fn optional_messages_map_to_progress_events() {
        let iteration_start = SingleTurnMode.progress_from_optional(
            OptionalSingleTurnMessage::IterationStart(IterationStart {
                iteration_number: 2,
                conversation_ids: vec![1, 2],
            }),
        );
        assert!(matches!(
            iteration_start.as_slice(),
            [SingleTurnProgressIndicatorMessage::IterationStart(start)]
                if start.iteration_number == 2 && start.conversation_ids == vec![1, 2]
        ));

        let iteration_complete = SingleTurnMode.progress_from_optional(
            OptionalSingleTurnMessage::IterationComplete(IterationComplete {
                iteration_number: 2,
                passed_conversation_ids: vec![1],
                failed_conversation_ids: vec![2],
            }),
        );
        assert!(matches!(
            iteration_complete.as_slice(),
            [SingleTurnProgressIndicatorMessage::IterationComplete(complete)]
                if complete.iteration_number == 2
                    && complete.passed_conversation_ids == vec![1]
                    && complete.failed_conversation_ids == vec![2]
        ));

        let conversation_complete = SingleTurnMode.progress_from_optional(
            OptionalSingleTurnMessage::ConversationComplete(ConversationComplete {
                conversation_id: 4,
                turns: 1,
                passed: true,
            }),
        );
        assert!(matches!(
            conversation_complete.as_slice(),
            [SingleTurnProgressIndicatorMessage::ConversationComplete(complete)]
                if complete.conversation_id == 4 && complete.passed
        ));

        let conversation_error = SingleTurnMode.progress_from_optional(
            OptionalSingleTurnMessage::ConversationError(ConversationError {
                conversation_id: 5,
                error_message: "boom".to_string(),
            }),
        );
        assert!(matches!(
            conversation_error.as_slice(),
            [SingleTurnProgressIndicatorMessage::ConversationError(error)]
                if error.conversation_id == 5 && error.error_message == "boom"
        ));
    }

    #[tokio::test]
    async fn run_evaluation_handles_concurrent_completion_requests() {
        let (first_behavior, first_sender) = gated_behavior();
        let (second_behavior, second_sender) = gated_behavior();
        let provider = Arc::new(ControlledProvider::new(HashMap::from([
            ("first".to_string(), first_behavior),
            ("second".to_string(), second_behavior),
        ])));

        let (websocket, server_handle) =
            spawn_websocket_server(move |server_websocket| async move {
                let (mut write, mut read) = server_websocket.split();

                let initial_request = recv_text_json(&mut read).await;
                assert_eq!(initial_request["type"], "single_turn_request");

                send_json(
                    &mut write,
                    json!({
                        "type": "completion_request",
                        "data": {
                            "request_id": "req-1",
                            "conversation_id": 1,
                            "messages": [{ "role": "user", "content": "first" }]
                        }
                    }),
                )
                .await;
                send_json(
                    &mut write,
                    json!({
                        "type": "completion_request",
                        "data": {
                            "request_id": "req-2",
                            "conversation_id": 2,
                            "messages": [{ "role": "user", "content": "second" }]
                        }
                    }),
                )
                .await;

                second_sender
                    .send(Ok("second reply".to_string()))
                    .expect("second provider result should send");
                let second_response = recv_text_json(&mut read).await;
                assert_eq!(second_response["type"], "completion_response");
                assert_eq!(second_response["data"]["request_id"], "req-2");
                assert_eq!(second_response["data"]["model_response"], "second reply");

                first_sender
                    .send(Ok("first reply".to_string()))
                    .expect("first provider result should send");
                let first_response = recv_text_json(&mut read).await;
                assert_eq!(first_response["type"], "completion_response");
                assert_eq!(first_response["data"]["request_id"], "req-1");
                assert_eq!(first_response["data"]["model_response"], "first reply");

                send_json(
                    &mut write,
                    json!({
                        "type": "single_turn_response",
                        "data": {
                            "total_passed": 2,
                            "total_failed": 0,
                            "failed_results": []
                        }
                    }),
                )
                .await;
            })
            .await;

        let response = run_evaluation(
            websocket,
            provider,
            SingleTurnRequest {
                threshold: 0.5,
                variations: 2,
                maximum_iteration_layers: 1,
                test_case_groups: vec!["suicidal_ideation".to_string()],
            },
            None,
        )
        .await
        .expect("single-turn evaluation should succeed");

        server_handle
            .await
            .expect("single-turn test server should finish");

        assert_eq!(response.total_passed, 2);
        assert_eq!(response.total_failed, 0);
        assert!(response.failed_results.is_empty());
    }

    #[tokio::test]
    async fn run_evaluation_closes_on_malformed_message() {
        let provider = Arc::new(ControlledProvider::new(HashMap::new()));
        let (websocket, server_handle) = spawn_websocket_server(|server_websocket| async move {
            let (mut write, mut read) = server_websocket.split();

            let initial_request = recv_text_json(&mut read).await;
            assert_eq!(initial_request["type"], "single_turn_request");

            write
                .send(Message::Text("{not-json".into()))
                .await
                .expect("server should send malformed payload");

            match read
                .next()
                .await
                .expect("client should respond with a close frame")
            {
                Ok(Message::Close(Some(frame))) => {
                    assert_eq!(frame.code, CloseCode::Protocol);
                    assert!(frame.reason.contains("Failed to parse incoming message"));
                }
                other => panic!("expected close frame, got {other:?}"),
            }
        })
        .await;

        let error = run_evaluation(
            websocket,
            provider,
            SingleTurnRequest {
                threshold: 0.5,
                variations: 2,
                maximum_iteration_layers: 1,
                test_case_groups: vec!["suicidal_ideation".to_string()],
            },
            None,
        )
        .await
        .expect_err("malformed message should fail the evaluation");

        server_handle
            .await
            .expect("single-turn malformed-message server should finish");

        assert!(matches!(
            error,
            EvaluationError::WebSocketClosed(message)
                if message.contains("Failed to parse incoming message")
        ));
    }

    #[tokio::test]
    async fn run_evaluation_returns_final_response_after_completion_error() {
        let provider = Arc::new(ControlledProvider::new(HashMap::from([(
            "broken".to_string(),
            ProviderBehavior::Immediate(Err(ProviderError::Network("timeout".to_string()))),
        )])));

        let (websocket, server_handle) = spawn_websocket_server(|server_websocket| async move {
            let (mut write, mut read) = server_websocket.split();

            let initial_request = recv_text_json(&mut read).await;
            assert_eq!(initial_request["type"], "single_turn_request");

            send_json(
                &mut write,
                json!({
                    "type": "completion_request",
                    "data": {
                        "request_id": "req-err",
                        "conversation_id": 7,
                        "messages": [{ "role": "user", "content": "broken" }]
                    }
                }),
            )
            .await;

            let completion_error = recv_text_json(&mut read).await;
            assert_eq!(completion_error["type"], "completion_error");
            assert_eq!(completion_error["data"]["request_id"], "req-err");
            assert_eq!(
                completion_error["data"]["error_reason"],
                "model_unreachable"
            );

            send_json(
                &mut write,
                json!({
                    "type": "single_turn_response",
                    "data": {
                        "total_passed": 0,
                        "total_failed": 1,
                        "failed_results": []
                    }
                }),
            )
            .await;
        })
        .await;

        let response = run_evaluation(
            websocket,
            provider,
            SingleTurnRequest {
                threshold: 0.5,
                variations: 2,
                maximum_iteration_layers: 1,
                test_case_groups: vec!["suicidal_ideation".to_string()],
            },
            None,
        )
        .await
        .expect("evaluation should still finish after a completion error");

        server_handle
            .await
            .expect("single-turn completion-error server should finish");

        assert_eq!(response.total_passed, 0);
        assert_eq!(response.total_failed, 1);
    }

    #[tokio::test]
    async fn run_evaluation_maps_server_close_frames() {
        let provider = Arc::new(ControlledProvider::new(HashMap::new()));
        let (websocket, server_handle) = spawn_websocket_server(|server_websocket| async move {
            let (mut write, mut read) = server_websocket.split();

            let initial_request = recv_text_json(&mut read).await;
            assert_eq!(initial_request["type"], "single_turn_request");

            write
                .send(Message::Close(Some(CloseFrame {
                    code: CloseCode::Library(4001),
                    reason: "bad token".into(),
                })))
                .await
                .expect("server should send close frame");
        })
        .await;

        let error = run_evaluation(
            websocket,
            provider,
            SingleTurnRequest {
                threshold: 0.5,
                variations: 2,
                maximum_iteration_layers: 1,
                test_case_groups: vec!["suicidal_ideation".to_string()],
            },
            None,
        )
        .await
        .expect_err("server close should fail the evaluation");

        server_handle
            .await
            .expect("single-turn close-frame server should finish");

        assert!(matches!(
            error,
            EvaluationError::ServerClose {
                code: ServerErrorCode::Unauthorized,
                reason
            } if reason == "bad token"
        ));
    }
}
