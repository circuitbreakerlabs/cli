use super::super::common::Message;
use crate::protocol_types::ConversationId;
use serde::{Deserialize, Serialize};

/// Payload for `MultiTurnResponseEnvelope` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnResponse {
    /// Persisted evaluation run ID.
    pub evaluation_id: i64,
    /// Number of conversations that passed
    pub total_passed: i32,
    /// Number of conversations that failed
    pub total_failed: i32,
    /// All persisted multi-turn conversation results.
    #[serde(default)]
    pub results: Vec<MultiTurnEvaluationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnEvaluationResult {
    pub test_result_id: i64,
    pub test_case_id: Option<i64>,
    pub min_safe_response_score: f64,
    pub conversation_id: ConversationId,
    pub conversation: Vec<Message>,
    pub passed: bool,
}

/// Server returns final multi-turn evaluation results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnResponseEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Response payload
    pub data: MultiTurnResponse,
}

#[cfg(test)]
mod tests {
    use super::MultiTurnResponseEnvelope;
    use serde_json::json;

    #[test]
    fn multi_turn_response_envelope_deserializes_from_protocol_shape() {
        let value = json!({
            "type": "multi_turn_response",
            "data": {
                "evaluation_id": 202,
                "total_passed": 3,
                "total_failed": 1,
                "results": [
                    {
                        "test_result_id": 42,
                        "test_case_id": 7,
                        "min_safe_response_score": 0.25,
                        "conversation_id": 7,
                        "conversation": [
                            { "role": "user", "content": "hello" },
                            { "role": "assistant", "content": "hi" }
                        ],
                        "passed": false
                    }
                ]
            }
        });

        let envelope: MultiTurnResponseEnvelope =
            serde_json::from_value(value).expect("response envelope should deserialize");

        assert_eq!(envelope.message_type, "multi_turn_response");
        assert_eq!(envelope.data.evaluation_id, 202);
        assert_eq!(envelope.data.total_passed, 3);
        assert_eq!(envelope.data.total_failed, 1);
        assert_eq!(envelope.data.results.len(), 1);
        assert_eq!(envelope.data.results[0].test_result_id, 42);
        assert_eq!(envelope.data.results[0].test_case_id, Some(7));
        assert_eq!(envelope.data.results[0].conversation_id, 7);
        assert_eq!(envelope.data.results[0].conversation.len(), 2);
        assert!(!envelope.data.results[0].passed);
    }
}
