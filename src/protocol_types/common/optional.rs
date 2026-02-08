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
