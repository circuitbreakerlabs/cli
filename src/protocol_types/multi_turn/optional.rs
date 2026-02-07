use serde::{Deserialize, Serialize};

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
