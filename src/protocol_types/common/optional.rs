use serde::{Deserialize, Serialize};

/// Payload for `UnsafeMessage` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsafeMessageObject {
    /// Identifier for the completion request that failed safety checks
    pub request_id: String,
}

/// Server notifies client that a response was flagged as unsafe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsafeMessage {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Unsafe message payload
    pub object: UnsafeMessageObject,
}
