use crate::protocol_types::common::{ConversationComplete, ConversationError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OptionalMultiTurnMessage {
    ConversationComplete(ConversationComplete),
    MultiTurnEvaluationStart(MultiTurnEvaluationStart),
    ConversationError(ConversationError),
}

/// Payload for `MultiTurnEvaluationStartEnvelope` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnEvaluationStart {
    /// Conversation identifiers that will be evaluated
    pub conversation_ids: Vec<i32>,
}

/// Server indicates that it is starting a multi-turn evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnEvaluationStartEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Start payload
    pub data: MultiTurnEvaluationStart,
}
