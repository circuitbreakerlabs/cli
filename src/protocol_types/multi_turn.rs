//! Multi-turn WebSocket protocol message types.

use super::common::{Message, TestCaseGroup};
use serde::{Deserialize, Serialize};

/// Payload for multi_turn_request messages (Client -> Server).
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

/// Details of a failed multi-turn conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedMultiTurnResult {
    /// Lowest safety score in the conversation
    pub min_safe_response_score: f64,
    /// Unique identifier for this conversation across this request
    pub conversation_id: i32,
    /// Full conversation history in role/content format
    pub conversation: Vec<Message>,
}

/// Payload for multi_turn_response messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnResponseObject {
    /// Number of conversations that passed
    pub total_passed: i32,
    /// Number of conversations that failed
    pub total_failed: i32,
    /// Details of each failed conversation
    pub failed_results: Vec<FailedMultiTurnResult>,
}

/// Server returns final multi-turn evaluation results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnResponse {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Response payload
    pub object: MultiTurnResponseObject,
}

/// Payload for conversation_complete messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationCompleteObject {
    /// Identifier for the completed conversation
    pub conversation_id: i32,
}

/// Server indicates that a particular multi-turn conversation evaluation has finished.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationComplete {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Complete payload
    pub object: ConversationCompleteObject,
}
