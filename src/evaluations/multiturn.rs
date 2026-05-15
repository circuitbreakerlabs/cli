use super::EvaluationError;
use super::engine::{self, EvaluationMode, ParsedIncoming};
use crate::protocol_types::multi_turn::{
    CategorizedMultiTurnMessage, MultiTurnReceivableMessage, MultiTurnRequest,
    MultiTurnRequestEnvelope, MultiTurnResponse, OptionalMultiTurnMessage,
};
use crate::protocol_types::{self};
use crate::response_provider::ResponseProvider;
use crate::tui;
use crate::tui::MultiTurnProgressIndicatorMessage;
use crate::websockets::WebSocketConnection;

use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Clone, Copy)]
struct MultiTurnMode {
    max_turns: usize,
}

impl EvaluationMode for MultiTurnMode {
    type Request = MultiTurnRequest;
    type FinalResponse = MultiTurnResponse;
    type OptionalMessage = OptionalMultiTurnMessage;
    type ProgressMessage = MultiTurnProgressIndicatorMessage;

    fn serialize_request(&self, request: &Self::Request) -> Result<String, serde_json::Error> {
        serde_json::to_string(&MultiTurnRequestEnvelope::from(request.clone()))
    }

    fn parse_text(
        &self,
        text: &str,
    ) -> Result<ParsedIncoming<Self::FinalResponse, Self::OptionalMessage>, serde_json::Error> {
        MultiTurnReceivableMessage::try_from(text.as_bytes())
            .map(CategorizedMultiTurnMessage::from)
            .map(|message| match message {
                CategorizedMultiTurnMessage::CompletionRequest(request) => {
                    ParsedIncoming::CompletionRequest(request)
                }
                CategorizedMultiTurnMessage::MultiTurnResponse(response) => {
                    ParsedIncoming::FinalResponse(response)
                }
                CategorizedMultiTurnMessage::OptionalMultiTurnMessage(message) => {
                    ParsedIncoming::OptionalMessage(message)
                }
            })
    }

    fn progress_on_completion_start(
        &self,
        request: &protocol_types::CompletionRequest,
    ) -> Vec<Self::ProgressMessage> {
        vec![
            MultiTurnProgressIndicatorMessage::ConversationTurn {
                conversation_id: request.conversation_id,
            },
            MultiTurnProgressIndicatorMessage::WaitingFor {
                conversation_id: request.conversation_id,
                waiting_for: tui::WaitingFor::Provider,
            },
        ]
    }

    fn progress_on_completion_success(
        &self,
        request: &protocol_types::CompletionRequest,
    ) -> Vec<Self::ProgressMessage> {
        vec![
            MultiTurnProgressIndicatorMessage::ConversationTurn {
                conversation_id: request.conversation_id,
            },
            MultiTurnProgressIndicatorMessage::WaitingFor {
                conversation_id: request.conversation_id,
                waiting_for: tui::WaitingFor::API,
            },
        ]
    }

    fn progress_from_optional(&self, message: Self::OptionalMessage) -> Vec<Self::ProgressMessage> {
        match message {
            OptionalMultiTurnMessage::MultiTurnEvaluationStart(evaluation_start) => {
                tracing::info!(
                    "Received MultiTurnEvaluationStart message: conversation_ids={:?}",
                    evaluation_start.conversation_ids,
                );
                vec![MultiTurnProgressIndicatorMessage::EvaluationStart {
                    conversation_ids: evaluation_start.conversation_ids,
                    max_turns: self.max_turns,
                }]
            }
            OptionalMultiTurnMessage::ConversationComplete(conversation_complete) => {
                tracing::info!(
                    "Received ConversationComplete message: conversation_id={}, turns={}, passed={}",
                    conversation_complete.conversation_id,
                    conversation_complete.turns,
                    conversation_complete.passed,
                );
                vec![MultiTurnProgressIndicatorMessage::ConversationComplete(
                    conversation_complete,
                )]
            }
            OptionalMultiTurnMessage::ConversationError(conversation_error) => {
                tracing::error!(
                    "Received ConversationError message: conversation_id={}, error_message={}",
                    conversation_error.conversation_id,
                    conversation_error.error_message,
                );
                vec![MultiTurnProgressIndicatorMessage::ConversationError(
                    conversation_error,
                )]
            }
        }
    }

    fn evaluation_complete_progress(&self) -> Option<Self::ProgressMessage> {
        Some(MultiTurnProgressIndicatorMessage::EvaluationComplete)
    }

    fn expected_response_name(&self) -> &'static str {
        "MultiTurnResponse"
    }
}

pub async fn run_evaluation(
    websocket_connection: WebSocketConnection,
    provider: Arc<dyn ResponseProvider>,
    request: MultiTurnRequest,
    progress_indicator: Option<mpsc::Sender<MultiTurnProgressIndicatorMessage>>,
) -> Result<MultiTurnResponse, EvaluationError> {
    let mode = MultiTurnMode {
        max_turns: request.max_turns,
    };

    engine::run_evaluation(
        websocket_connection,
        provider,
        request,
        progress_indicator,
        mode,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::{EvaluationMode, MultiTurnMode, OptionalMultiTurnMessage, run_evaluation};
    use crate::evaluations::test_support::{
        ControlledProvider, ProviderBehavior, recv_text_json, send_json, spawn_websocket_server,
    };
    use crate::protocol_types::common::{ConversationComplete, ConversationError};
    use crate::protocol_types::multi_turn::{MultiTurnEvaluationStart, MultiTurnRequest};
    use crate::protocol_types::{self, Role};
    use crate::tui::MultiTurnProgressIndicatorMessage;
    use crate::tui::WaitingFor;
    use futures_util::{SinkExt, StreamExt};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio_tungstenite::tungstenite::Message;

    #[test]
    fn completion_progress_preserves_double_turn_increment() {
        let mode = MultiTurnMode { max_turns: 4 };
        let request = protocol_types::CompletionRequest {
            request_id: "req-1".to_string(),
            conversation_id: 12,
            messages: vec![protocol_types::Message {
                role: Role::User,
                content: "hello".to_string(),
            }],
        };

        let start = mode.progress_on_completion_start(&request);
        let success = mode.progress_on_completion_success(&request);

        assert!(matches!(
            start.as_slice(),
            [
                MultiTurnProgressIndicatorMessage::ConversationTurn {
                    conversation_id: 12
                },
                MultiTurnProgressIndicatorMessage::WaitingFor {
                    conversation_id: 12,
                    waiting_for: WaitingFor::Provider
                }
            ]
        ));
        assert!(matches!(
            success.as_slice(),
            [
                MultiTurnProgressIndicatorMessage::ConversationTurn {
                    conversation_id: 12
                },
                MultiTurnProgressIndicatorMessage::WaitingFor {
                    conversation_id: 12,
                    waiting_for: WaitingFor::API
                }
            ]
        ));
    }

    #[test]
    fn optional_messages_map_to_multiturn_progress_events() {
        let mode = MultiTurnMode { max_turns: 6 };

        let evaluation_start = mode.progress_from_optional(
            OptionalMultiTurnMessage::MultiTurnEvaluationStart(MultiTurnEvaluationStart {
                conversation_ids: vec![3, 4],
            }),
        );
        assert!(matches!(
            evaluation_start.as_slice(),
            [MultiTurnProgressIndicatorMessage::EvaluationStart {
                conversation_ids,
                max_turns: 6
            }] if conversation_ids == &vec![3, 4]
        ));

        let conversation_complete = mode.progress_from_optional(
            OptionalMultiTurnMessage::ConversationComplete(ConversationComplete {
                conversation_id: 8,
                turns: 3,
                passed: false,
            }),
        );
        assert!(matches!(
            conversation_complete.as_slice(),
            [MultiTurnProgressIndicatorMessage::ConversationComplete(complete)]
                if complete.conversation_id == 8 && complete.turns == 3 && !complete.passed
        ));

        let conversation_error = mode.progress_from_optional(
            OptionalMultiTurnMessage::ConversationError(ConversationError {
                conversation_id: 9,
                error_message: "bad".to_string(),
            }),
        );
        assert!(matches!(
            conversation_error.as_slice(),
            [MultiTurnProgressIndicatorMessage::ConversationError(error)]
                if error.conversation_id == 9 && error.error_message == "bad"
        ));
    }

    #[tokio::test]
    async fn run_evaluation_completes_multi_turn_happy_path() {
        let provider = Arc::new(ControlledProvider::new(HashMap::from([(
            "first turn".to_string(),
            ProviderBehavior::Immediate(Ok("assistant reply".to_string())),
        )])));

        let (websocket, server_handle) = spawn_websocket_server(|server_websocket| async move {
            let (mut write, mut read) = server_websocket.split();

            let initial_request = recv_text_json(&mut read).await;
            assert_eq!(initial_request["type"], "multi_turn_request");

            send_json(
                &mut write,
                json!({
                    "type": "multi_turn_evaluation_start",
                    "data": {
                        "conversation_ids": [1]
                    }
                }),
            )
            .await;
            send_json(
                &mut write,
                json!({
                    "type": "completion_request",
                    "data": {
                        "request_id": "req-1",
                        "conversation_id": 1,
                        "messages": [{ "role": "user", "content": "first turn" }]
                    }
                }),
            )
            .await;

            let completion_response = recv_text_json(&mut read).await;
            assert_eq!(completion_response["type"], "completion_response");
            assert_eq!(completion_response["data"]["request_id"], "req-1");
            assert_eq!(
                completion_response["data"]["model_response"],
                "assistant reply"
            );

            send_json(
                &mut write,
                json!({
                    "type": "multi_turn_response",
                    "data": {
                        "total_passed": 1,
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
            MultiTurnRequest {
                threshold: 0.5,
                max_turns: 4,
                test_case_groups: vec!["suicidal_ideation".to_string()],
            },
            None,
        )
        .await
        .expect("multi-turn evaluation should succeed");

        server_handle
            .await
            .expect("multi-turn test server should finish");

        assert_eq!(response.total_passed, 1);
        assert_eq!(response.total_failed, 0);
        assert!(response.failed_results.is_empty());
    }

    #[tokio::test]
    async fn run_evaluation_replies_to_ping_with_pong() {
        let provider = Arc::new(ControlledProvider::new(HashMap::new()));

        let (websocket, server_handle) = spawn_websocket_server(|server_websocket| async move {
            let (mut write, mut read) = server_websocket.split();

            let initial_request = recv_text_json(&mut read).await;
            assert_eq!(initial_request["type"], "multi_turn_request");

            write
                .send(Message::Ping(vec![9, 8, 7].into()))
                .await
                .expect("server should send ping");

            match read.next().await.expect("client should respond to ping") {
                Ok(Message::Pong(payload)) => assert_eq!(payload.to_vec(), vec![9, 8, 7]),
                other => panic!("expected pong message, got {other:?}"),
            }

            send_json(
                &mut write,
                json!({
                    "type": "multi_turn_response",
                    "data": {
                        "total_passed": 1,
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
            MultiTurnRequest {
                threshold: 0.5,
                max_turns: 4,
                test_case_groups: vec!["suicidal_ideation".to_string()],
            },
            None,
        )
        .await
        .expect("multi-turn evaluation should succeed after ping/pong");

        server_handle
            .await
            .expect("multi-turn ping server should finish");

        assert_eq!(response.total_passed, 1);
        assert_eq!(response.total_failed, 0);
    }
}
