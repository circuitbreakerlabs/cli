use super::{
    EvaluationError,
    engine::{self, EvaluationMode, ParsedIncoming},
};
use crate::{
    protocol_types::{
        CompletionRequest,
        multi_party::{
            MultiPartyEvaluationRequest, MultiPartyRequestEnvelope, MultiPartyRerunRequestEnvelope,
            MultiPartyResponse,
        },
    },
    response_provider::ResponseProvider,
    tui::{WaitingFor, multiparty::MultiPartyProgressIndicatorMessage},
    websockets::WebSocketConnection,
};
use serde::Deserialize;
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Clone, Copy)]
struct Mode;
#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    kind: String,
    data: Value,
}

impl EvaluationMode for Mode {
    type Request = MultiPartyEvaluationRequest;
    type FinalResponse = MultiPartyResponse;
    type OptionalMessage = MultiPartyProgressIndicatorMessage;
    type ProgressMessage = MultiPartyProgressIndicatorMessage;
    fn serialize_request(&self, request: &Self::Request) -> Result<String, serde_json::Error> {
        match request {
            MultiPartyEvaluationRequest::Standard(request) => {
                serde_json::to_string(&MultiPartyRequestEnvelope::from(request.clone()))
            }
            MultiPartyEvaluationRequest::Rerun(request) => {
                serde_json::to_string(&MultiPartyRerunRequestEnvelope::from(request.clone()))
            }
        }
    }
    fn parse_text(
        &self,
        text: &str,
    ) -> Result<ParsedIncoming<Self::FinalResponse, Self::OptionalMessage>, serde_json::Error> {
        let envelope: Envelope = serde_json::from_str(text)?;
        match envelope.kind.as_str() {
            "completion_request" => Ok(ParsedIncoming::CompletionRequest(serde_json::from_value(
                envelope.data,
            )?)),
            "multiparty_response" => Ok(ParsedIncoming::FinalResponse(serde_json::from_value(
                envelope.data,
            )?)),
            "multiparty_evaluation_start"
            | "multiparty_conversation_start"
            | "multiparty_turn"
            | "multiparty_conversation_complete" => {
                Ok(ParsedIncoming::OptionalMessage(serde_json::from_str(text)?))
            }
            _ => Ok(ParsedIncoming::OptionalMessage(
                MultiPartyProgressIndicatorMessage::Ignored,
            )),
        }
    }
    fn progress_on_completion_start(
        &self,
        request: &CompletionRequest,
    ) -> Vec<Self::ProgressMessage> {
        vec![MultiPartyProgressIndicatorMessage::WaitingFor {
            conversation_id: request.conversation_id,
            waiting_for: WaitingFor::Provider,
        }]
    }
    fn progress_on_completion_success(
        &self,
        request: &CompletionRequest,
    ) -> Vec<Self::ProgressMessage> {
        vec![MultiPartyProgressIndicatorMessage::WaitingFor {
            conversation_id: request.conversation_id,
            waiting_for: WaitingFor::API,
        }]
    }
    fn progress_from_optional(&self, message: Self::OptionalMessage) -> Vec<Self::ProgressMessage> {
        if matches!(message, MultiPartyProgressIndicatorMessage::Ignored) {
            return vec![];
        }
        tracing::info!(event = ?message, "Multi-party progress");
        vec![message]
    }
    fn evaluation_complete_progress(&self) -> Option<Self::ProgressMessage> {
        Some(MultiPartyProgressIndicatorMessage::EvaluationComplete)
    }
    fn expected_response_name(&self) -> &'static str {
        "MultiPartyResponse"
    }
}

pub async fn run_evaluation(
    websocket: WebSocketConnection,
    provider: Arc<dyn ResponseProvider>,
    request: MultiPartyEvaluationRequest,
    progress: Option<mpsc::Sender<MultiPartyProgressIndicatorMessage>>,
) -> Result<MultiPartyResponse, EvaluationError> {
    engine::run_evaluation(websocket, provider, request, progress, Mode).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluations::test_support::{
        ControlledProvider, ProviderBehavior, recv_text_json, send_json, spawn_websocket_server,
    };
    use futures_util::StreamExt;
    use std::collections::HashMap;

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn multiparty_routes_completions_and_parses_sanitized_results() {
        let (websocket, server) = spawn_websocket_server(|socket| async move {
            let (mut write, mut read) = socket.split();
            let initial = recv_text_json(&mut read).await;
            assert_eq!(initial["data"]["test_case_groups"], json!(["pack"]));
            assert_eq!(initial["data"]["max_concurrent_test_case_executions"], 2);
            assert!(initial["data"].get("capture_trace").is_none());
            send_json(&mut write, json!({"type":"multiparty_evaluation_start","data":{"conversation_ids":[1,2],"max_turns":7}})).await;
            for id in [1, 2] {
                send_json(&mut write, json!({"type":"completion_request", "data":{"request_id":format!("r{id}"), "conversation_id":id, "messages":[{"role":"user","content":format!("history-{id}")}]}})).await;
            }
            let mut replies = HashMap::new();
            for _ in 0..2 {
                let reply = recv_text_json(&mut read).await;
                replies.insert(reply["data"]["request_id"].as_str().unwrap().to_owned(), reply["data"]["model_response"].clone());
            }
            assert_eq!(replies["r1"], "answer-1");
            assert_eq!(replies["r2"], "answer-2");
            send_json(&mut write, json!({"type":"multiparty_turn","data":{"conversation_id":1,"turns":5}})).await;
            send_json(&mut write, json!({"type":"multiparty_conversation_complete","data":{"conversation_id":1,"status":"error"}})).await;
            send_json(&mut write, json!({"type":"multiparty_response","data":{"evaluation_id":42,"total_passed":0,"total_failed":0,"total_errors":1,"results":[{"conversation_id":1,"multiparty_test_case_id":7,"status":"error","turns":1,"min_safe_response_score":null,"passed":null,"conversation":[{"role":"user","name":"Alex","content":"partial"}],"participant_results":[{"participant_id":"alex","test_case_id":9,"test_result_id":null,"status":"error","safe_response_score":null,"passed":null}],"error_code":"evaluation_failure"}]}})).await;
        }).await;
        let provider = Arc::new(ControlledProvider::new(HashMap::from([
            (
                "history-1".into(),
                ProviderBehavior::Immediate(Ok("answer-1".into())),
            ),
            (
                "history-2".into(),
                ProviderBehavior::Immediate(Ok("answer-2".into())),
            ),
        ])));
        let (tx, mut rx) = mpsc::channel(32);
        let result = run_evaluation(
            websocket,
            provider,
            MultiPartyEvaluationRequest::Standard(
                crate::protocol_types::multi_party::MultiPartyRequest {
                    test_case_groups: vec!["pack".into()],
                    max_concurrent_test_case_executions: 2,
                    threshold: 0.5,
                    max_turns: 7,
                },
            ),
            Some(tx),
        )
        .await
        .unwrap();
        assert_eq!(result.evaluation_id, 42);
        assert_eq!(result.total_errors, 1);
        assert_eq!(
            result.results[0].error_code,
            Some(crate::protocol_types::multi_party::MultiPartyErrorCode::EvaluationFailure)
        );
        assert_eq!(
            result.results[0].conversation[0].name.as_deref(),
            Some("Alex")
        );
        assert_eq!(
            result.results[0].participant_results[0].participant_id,
            "alex"
        );
        let mut messages = Vec::new();
        while let Some(message) = rx.recv().await {
            messages.push(message);
        }
        assert!(matches!(
            messages.first(),
            Some(MultiPartyProgressIndicatorMessage::EvaluationStart { max_turns: 7, .. })
        ));
        assert_eq!(
            messages
                .iter()
                .filter(|message| matches!(
                    message,
                    MultiPartyProgressIndicatorMessage::WaitingFor {
                        waiting_for: WaitingFor::Provider,
                        ..
                    }
                ))
                .count(),
            2
        );
        assert_eq!(
            messages
                .iter()
                .filter(|message| matches!(
                    message,
                    MultiPartyProgressIndicatorMessage::ConversationTurn { turns: 5, .. }
                ))
                .count(),
            1
        );
        assert!(matches!(
            messages.last(),
            Some(MultiPartyProgressIndicatorMessage::EvaluationComplete)
        ));
        assert!(messages.iter().any(|message| matches!(
            message,
            MultiPartyProgressIndicatorMessage::ConversationComplete {
                conversation_id: 1,
                status: crate::tui::multiparty::ConversationOutcome::Error,
            }
        )));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn multiparty_rerun_sends_rerun_envelope_and_parses_response() {
        let (websocket, server) = spawn_websocket_server(|socket| async move {
            let (mut write, mut read) = socket.split();
            let initial = recv_text_json(&mut read).await;
            assert_eq!(initial["type"], "multiparty_rerun_request");
            assert_eq!(initial["version"], crate::consts::version::PROTOCOL_VERSION);
            assert_eq!(initial["data"]["test_result_ids"], json!([42, 43]));
            assert_eq!(initial["data"]["threshold"], 0.75);
            assert_eq!(initial["data"]["max_turns"], 9);
            assert!(initial["data"].get("test_case_groups").is_none());
            send_json(
                &mut write,
                json!({
                    "type": "multiparty_response",
                    "data": {
                        "evaluation_id": 99,
                        "total_passed": 0,
                        "total_failed": 0,
                        "total_errors": 0,
                        "results": []
                    }
                }),
            )
            .await;
        })
        .await;
        let result = run_evaluation(
            websocket,
            Arc::new(ControlledProvider::new(HashMap::new())),
            MultiPartyEvaluationRequest::Rerun(
                crate::protocol_types::multi_party::MultiPartyRerunRequest {
                    selector: crate::protocol_types::common::RerunSelector::TestResultIds {
                        test_result_ids: vec![42, 43],
                    },
                    max_concurrent_test_case_executions: 2,
                    threshold: 0.75,
                    max_turns: 9,
                },
            ),
            None,
        )
        .await
        .unwrap();
        assert_eq!(result.evaluation_id, 99);
        server.await.unwrap();
    }

    #[test]
    fn ignores_future_progress_events_and_rejects_malformed_known_events() {
        let ParsedIncoming::OptionalMessage(message) = Mode
            .parse_text(r#"{"type":"future_multiparty_event","data":{"detail":"new"}}"#)
            .unwrap()
        else {
            panic!("expected optional event");
        };
        assert!(Mode.progress_from_optional(message).is_empty());
        assert!(
            Mode.parse_text(r#"{"type":"multiparty_turn","data":{"conversation_id":1}}"#)
                .is_err()
        );
    }
}
