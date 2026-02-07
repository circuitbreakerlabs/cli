use super::super::common::Message;
use serde::{Deserialize, Serialize};

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
