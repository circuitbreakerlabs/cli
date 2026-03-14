use crate::protocol_types::ConversationId;
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
    pub conversation_ids: Vec<ConversationId>,
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

#[cfg(test)]
mod tests {
    use super::MultiTurnEvaluationStartEnvelope;
    use serde_json::json;

    #[test]
    fn multi_turn_evaluation_start_envelope_deserializes_from_protocol_shape() {
        let value = json!({
            "type": "multi_turn_evaluation_start",
            "data": {
                "conversation_ids": [10, 11, 12]
            }
        });

        let envelope: MultiTurnEvaluationStartEnvelope =
            serde_json::from_value(value).expect("evaluation start should deserialize");

        assert_eq!(envelope.message_type, "multi_turn_evaluation_start");
        assert_eq!(envelope.data.conversation_ids, vec![10, 11, 12]);
    }
}
