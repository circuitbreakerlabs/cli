use serde::{Deserialize, Serialize};

use crate::protocol_types::ConversationId;

/// Details of a failed single-turn test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedSingleTurnResult {
    pub test_result_id: Option<i64>,
    pub test_case_id: Option<i64>,
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
    /// Persisted evaluation run ID.
    pub evaluation_id: i64,
    /// Number of test cases that passed
    pub total_passed: i32,
    /// Number of test cases that failed
    pub total_failed: i32,
    /// Details of each failed test case per iteration layer
    pub failed_results: Vec<Vec<FailedSingleTurnResult>>,
    #[serde(default)]
    pub results_by_iteration: Vec<Vec<SingleTurnEvaluationResult>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnEvaluationResult {
    pub test_result_id: i64,
    pub test_case_id: Option<i64>,
    pub user_input: String,
    pub conversation_id: ConversationId,
    pub model_response: String,
    pub safe_response_score: f64,
    pub passed: bool,
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
                "evaluation_id": 101,
                "total_passed": 5,
                "total_failed": 1,
                "failed_results": [
                    [
                        {
                            "test_result_id": 42,
                            "test_case_id": 7,
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
        assert_eq!(envelope.data.evaluation_id, 101);
        assert_eq!(envelope.data.total_passed, 5);
        assert_eq!(envelope.data.total_failed, 1);
        assert_eq!(envelope.data.failed_results.len(), 1);
        assert_eq!(envelope.data.failed_results[0][0].test_result_id, Some(42));
        assert_eq!(envelope.data.failed_results[0][0].test_case_id, Some(7));
        assert_eq!(envelope.data.failed_results[0][0].conversation_id, 2);
        assert_eq!(
            envelope.data.failed_results[0][0].model_response,
            "unsafe reply"
        );
    }
}
