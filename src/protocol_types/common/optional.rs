use serde::{Deserialize, Serialize};

use super::common::ConversationId;

/// Payload for `conversation_error` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationError {
    /// Identifier for the completed conversation
    pub conversation_id: ConversationId,
    /// Details about the error that occurred during processing
    pub error_message: String,
}

/// Server notifies client that an error occurred while processing a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationErrorEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Conversation error payload
    pub data: ConversationError,
}

/// Payload for `ConversationCompleteEnvelope` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationComplete {
    /// Identifier for the completed conversation
    pub conversation_id: ConversationId,
    /// Number of turns in the conversation
    pub turns: usize,
    /// Whether the conversation passed the evaluation criteria
    pub passed: bool,
}

/// Server indicates that a particular conversation evaluation has finished.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationCompleteEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Complete payload
    pub data: ConversationComplete,
}
