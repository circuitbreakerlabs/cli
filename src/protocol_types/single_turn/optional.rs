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

#[cfg(test)]
mod tests {
    use super::{IterationCompleteEnvelope, IterationStartEnvelope};
    use serde_json::json;

    #[test]
    fn iteration_start_envelope_deserializes_from_protocol_shape() {
        let value = json!({
            "type": "iteration_start",
            "data": {
                "iteration_number": 1,
                "conversation_ids": [1, 2, 3]
            }
        });

        let envelope: IterationStartEnvelope =
            serde_json::from_value(value).expect("iteration start should deserialize");

        assert_eq!(envelope.message_type, "iteration_start");
        assert_eq!(envelope.data.iteration_number, 1);
        assert_eq!(envelope.data.conversation_ids, vec![1, 2, 3]);
    }

    #[test]
    fn iteration_complete_envelope_deserializes_from_protocol_shape() {
        let value = json!({
            "type": "iteration_complete",
            "data": {
                "iteration_number": 2,
                "passed_conversation_ids": [1, 3],
                "failed_conversation_ids": [2]
            }
        });

        let envelope: IterationCompleteEnvelope =
            serde_json::from_value(value).expect("iteration complete should deserialize");

        assert_eq!(envelope.message_type, "iteration_complete");
        assert_eq!(envelope.data.iteration_number, 2);
        assert_eq!(envelope.data.passed_conversation_ids, vec![1, 3]);
        assert_eq!(envelope.data.failed_conversation_ids, vec![2]);
    }
}
