use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OptionalMultiTurnMessage {
    ConversationComplete(ConversationComplete),
    MultiTurnEvaluationStart(MultiTurnEvaluationStart),
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

/// Server indicates that a particular multi-turn conversation evaluation has finished.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationCompleteEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Complete payload
    pub data: ConversationComplete,
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
