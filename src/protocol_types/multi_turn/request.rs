use super::super::common::TestCaseGroup;
use serde::{Deserialize, Serialize};

/// Payload for `MultiTurnRequest` messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnRequestObject {
    /// Safety threshold parameter
    pub threshold: i32,
    /// Maximum conversation turns to evaluate
    pub max_turns: i32,
    /// Test case categories to evaluate
    pub test_case_groups: Vec<TestCaseGroup>,
    /// Types of multi-turn tests to run
    pub test_types: Vec<String>,
}

/// Client initiates a multi-turn conversational evaluation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnRequest {
    /// Protocol version
    pub version: String,
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Request payload
    pub object: MultiTurnRequestObject,
}
