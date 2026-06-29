use super::super::common::Message;
use crate::protocol_types::ConversationId;
use serde::{Deserialize, Serialize};

/// Details of a failed multi-turn conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedMultiTurnResult {
    pub test_result_id: Option<i64>,
    pub test_case_id: Option<i64>,
    /// Lowest safety score in the conversation
    pub min_safe_response_score: f64,
    /// Unique identifier for this conversation across this request
    pub conversation_id: ConversationId,
    /// Full conversation history in role/content format
    pub conversation: Vec<Message>,
}

/// Payload for `MultiTurnResponseEnvelope` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnResponse {
    /// Persisted evaluation run ID.
    pub evaluation_id: i64,
    /// Number of conversations that passed
    pub total_passed: i32,
    /// Number of conversations that failed
    pub total_failed: i32,
    /// Details of each failed conversation
    pub failed_results: Vec<FailedMultiTurnResult>,
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
                "failed_results": [
                    {
                        "test_result_id": 42,
                        "test_case_id": 7,
                        "min_safe_response_score": 0.25,
                        "conversation_id": 7,
                        "conversation": [
                            { "role": "user", "content": "hello" },
                            { "role": "assistant", "content": "hi" }
                        ]
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
        assert_eq!(envelope.data.failed_results.len(), 1);
        assert_eq!(envelope.data.failed_results[0].test_result_id, Some(42));
        assert_eq!(envelope.data.failed_results[0].test_case_id, Some(7));
        assert_eq!(envelope.data.failed_results[0].conversation_id, 7);
        assert_eq!(envelope.data.failed_results[0].conversation.len(), 2);
    }
}
