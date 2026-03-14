use serde::{Deserialize, Serialize};

use crate::protocol_types::ConversationId;

/// Details of a failed single-turn test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedSingleTurnResult {
    /// The prompt that was tested
    pub user_input: String,
    /// Unique identifier for this conversation across this request
    pub conversation_id: ConversationId,
    /// The model's response
    pub model_response: String,
    /// Safety score (lower indicates less safe)
    pub safe_response_score: f64,
}

/// Payload for `SingleTurnResponseEnvelope` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnResponse {
    /// Number of test cases that passed
    pub total_passed: i32,
    /// Number of test cases that failed
    pub total_failed: i32,
    /// Details of each failed test case per iteration layer
    pub failed_results: Vec<Vec<FailedSingleTurnResult>>,
}

/// Server returns final single-turn evaluation results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnResponseEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Response payload
    pub data: SingleTurnResponse,
}

#[cfg(test)]
mod tests {
    use super::SingleTurnResponseEnvelope;
    use serde_json::json;

    #[test]
    fn single_turn_response_envelope_deserializes_from_protocol_shape() {
        let value = json!({
            "type": "single_turn_response",
            "data": {
                "total_passed": 5,
                "total_failed": 1,
                "failed_results": [
                    [
                        {
                            "user_input": "unsafe prompt",
                            "conversation_id": 2,
                            "model_response": "unsafe reply",
                            "safe_response_score": 0.12
                        }
                    ]
                ]
            }
        });

        let envelope: SingleTurnResponseEnvelope =
            serde_json::from_value(value).expect("response envelope should deserialize");

        assert_eq!(envelope.message_type, "single_turn_response");
        assert_eq!(envelope.data.total_passed, 5);
        assert_eq!(envelope.data.total_failed, 1);
        assert_eq!(envelope.data.failed_results.len(), 1);
        assert_eq!(envelope.data.failed_results[0][0].conversation_id, 2);
        assert_eq!(
            envelope.data.failed_results[0][0].model_response,
            "unsafe reply"
        );
    }
}
