use serde::{Deserialize, Serialize};

use super::super::common::TestCaseGroup;

/// Payload for `SingleTurnRequestEnvelope` messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnRequest {
    /// Safety threshold parameter
    pub threshold: i32,
    /// Maximum number of evaluation iterations
    pub maximum_iteration_layers: i32,
    /// Test case categories to evaluate
    pub test_case_groups: Vec<TestCaseGroup>,
}

/// Client initiates a single-turn evaluation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnRequestEnvelope {
    /// Protocol version
    pub version: String,
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Request payload
    pub data: SingleTurnRequest,
}
