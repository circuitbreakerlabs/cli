use serde::{Deserialize, Serialize};

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
