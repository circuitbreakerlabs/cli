use serde::{Deserialize, Serialize};

/// Details of a failed single-turn test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedSingleTurnResult {
    /// The prompt that was tested
    pub user_input: String,
    /// The model's response
    pub model_response: String,
    /// Safety score (lower indicates less safe)
    pub safe_response_score: f64,
}

/// Payload for `SingleTurnResponse` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnResponseObject {
    /// Number of test cases that passed
    pub total_passed: i32,
    /// Number of test cases that failed
    pub total_failed: i32,
    /// Details of each failed test case
    pub failed_results: Vec<FailedSingleTurnResult>,
}

/// Server returns final single-turn evaluation results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnResponse {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Response payload
    pub object: SingleTurnResponseObject,
}
