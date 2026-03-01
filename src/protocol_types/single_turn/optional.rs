use crate::protocol_types::ConversationId;
use crate::protocol_types::common::{ConversationComplete, ConversationError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OptionalSingleTurnMessage {
    IterationStart(IterationStart),
    IterationComplete(IterationComplete),
    ConversationComplete(ConversationComplete),
    ConversationError(ConversationError),
}

/// Payload for `IterationStartEnvelope` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationStart {
    /// Current iteration index
    pub iteration_number: i32,
    /// Conversation identifiers that will be evaluated in this iteration
    pub conversation_ids: Vec<ConversationId>,
}

/// Server indicates the start of a new evaluation iteration/layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationStartEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Start payload
    pub data: IterationStart,
}

/// Payload for `IterationCompleteEnvelope` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationComplete {
    /// Completed iteration index
    pub iteration_number: i32,
    /// Conversation identifiers that passed in this iteration
    pub passed_conversation_ids: Vec<ConversationId>,
    /// Conversation identifiers that failed in this iteration
    pub failed_conversation_ids: Vec<ConversationId>,
}

/// Server indicates completion of an evaluation iteration/layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationCompleteEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Complete payload
    pub data: IterationComplete,
}
