use serde::{Deserialize, Serialize};

/// Payload for `UnsafeMessageEnvelope` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsafeMessage {
    /// Identifier for the completion request that failed safety checks
    pub request_id: String,
}

/// Server notifies client that a response was flagged as unsafe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsafeMessageEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Unsafe message payload
    pub data: UnsafeMessage,
}

/// Payload for conversation_error messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationError {
    /// Identifier for the completed conversation
    pub conversation_id: i32,
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
