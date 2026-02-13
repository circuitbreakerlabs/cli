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

/// Payload for `ConversationCompleteEnvelope` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationComplete {
    /// Identifier for the completed conversation
    pub conversation_id: i32,
    /// Number of turns in the conversation
    pub turns: i32,
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
