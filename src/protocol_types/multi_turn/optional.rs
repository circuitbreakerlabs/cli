use serde::{Deserialize, Serialize};

/// Payload for `ConversationCompleteEnvelope` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationComplete {
    /// Identifier for the completed conversation
    pub conversation_id: i32,
}

/// Server indicates that a particular multi-turn conversation evaluation has finished.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationCompleteEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Complete payload
    pub data: ConversationComplete,
}
