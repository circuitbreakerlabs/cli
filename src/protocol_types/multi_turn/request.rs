use super::super::common::TestCaseGroup;
use serde::{Deserialize, Serialize};
use strum::EnumString;

#[derive(Debug, Clone, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "snake_case")]
pub enum MultiTurnTestType {
    UserPersona,
    SemanticChunks,
}

/// Payload for `MultiTurnRequestEnvelope` messages (Client -> Server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnRequest {
    /// Safety threshold parameter
    pub threshold: f32,
    /// Maximum conversation turns to evaluate
    pub max_turns: i32,
    /// Test case categories to evaluate
    pub test_case_groups: Vec<TestCaseGroup>,
    /// Types of multi-turn tests to run
    pub test_types: Vec<MultiTurnTestType>,
}

/// Client initiates a multi-turn conversational evaluation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnRequestEnvelope {
    /// Protocol version
    pub version: String,
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Request payload
    pub data: MultiTurnRequest,
}
