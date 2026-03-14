use serde::{Deserialize, Serialize};

use super::common::ConversationId;

/// Payload for `conversation_error` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationError {
    /// Identifier for the completed conversation
    pub conversation_id: ConversationId,
    /// Details about the error that occurred during processing
    pub error_message: String,
}

/// Server notifies client that an error occurred while processing a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationErrorEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Conversation error payload
    pub data: ConversationError,
}

/// Payload for `ConversationCompleteEnvelope` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationComplete {
    /// Identifier for the completed conversation
    pub conversation_id: ConversationId,
    /// Number of turns in the conversation
    pub turns: usize,
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

#[cfg(test)]
mod tests {
    use super::{ConversationCompleteEnvelope, ConversationErrorEnvelope};
    use serde_json::json;

    #[test]
    fn conversation_error_envelope_deserializes_from_protocol_shape() {
        let value = json!({
            "type": "conversation_error",
            "data": {
                "conversation_id": 4,
                "error_message": "provider timed out"
            }
        });

        let envelope: ConversationErrorEnvelope =
            serde_json::from_value(value).expect("error envelope should deserialize");

        assert_eq!(envelope.message_type, "conversation_error");
        assert_eq!(envelope.data.conversation_id, 4);
        assert_eq!(envelope.data.error_message, "provider timed out");
    }

    #[test]
    fn conversation_complete_envelope_deserializes_from_protocol_shape() {
        let value = json!({
            "type": "conversation_complete",
            "data": {
                "conversation_id": 8,
                "turns": 6,
                "passed": true
            }
        });

        let envelope: ConversationCompleteEnvelope =
            serde_json::from_value(value).expect("complete envelope should deserialize");

        assert_eq!(envelope.message_type, "conversation_complete");
        assert_eq!(envelope.data.conversation_id, 8);
        assert_eq!(envelope.data.turns, 6);
        assert!(envelope.data.passed);
    }
}
