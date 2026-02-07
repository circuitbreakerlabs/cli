//! Single-turn WebSocket protocol message types.

use serde::{Deserialize, Serialize};

use super::common::TestCaseGroup;

/// Payload for single_turn_request messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnRequestObject {
    /// Safety threshold parameter
    pub threshold: i32,
    /// Maximum number of evaluation iterations
    pub maximum_iteration_layers: i32,
    /// Test case categories to evaluate
    pub test_case_groups: Vec<TestCaseGroup>,
}

/// Client initiates a single-turn evaluation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnRequest {
    /// Protocol version
    pub version: String,
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Request payload
    pub object: SingleTurnRequestObject,
}

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

/// Payload for single_turn_response messages (Server -> Client).
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

/// Payload for iteration_start messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationStartObject {
    /// Current iteration index
    pub iteration_number: i32,
    /// Number of test cases in this iteration
    pub total_test_cases: i32,
}

/// Server indicates the start of a new evaluation iteration/layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationStart {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Start payload
    pub object: IterationStartObject,
}

/// Payload for iteration_complete messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationCompleteObject {
    /// Completed iteration index
    pub iteration_number: i32,
    /// Test cases passed in this iteration
    pub total_passed: i32,
    /// Test cases failed in this iteration
    pub total_failed: i32,
}

/// Server indicates completion of an evaluation iteration/layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationComplete {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Complete payload
    pub object: IterationCompleteObject,
}
