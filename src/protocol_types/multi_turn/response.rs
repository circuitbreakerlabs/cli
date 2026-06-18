use super::super::common::Message;
use crate::protocol_types::ConversationId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Details of a failed multi-turn conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedMultiTurnResult {
    /// Lowest safety score in the conversation
    pub min_safe_response_score: f64,
    /// Unique identifier for this conversation across this request
    pub conversation_id: ConversationId,
    /// Full conversation history in role/content format
    pub conversation: Vec<Message>,
}

/// Payload for `MultiTurnResponseEnvelope` messages (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnResponse {
    /// Number of conversations that passed
    pub total_passed: i32,
    /// Number of conversations that failed
    pub total_failed: i32,
    /// Details of each failed conversation
    pub failed_results: Vec<FailedMultiTurnResult>,
}

/// Server returns final multi-turn evaluation results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnResponseEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Response payload
    pub data: MultiTurnResponse,
}

/// Payload for parallel multi-turn evaluation results keyed by target model ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelMultiTurnResponse {
    pub results: HashMap<String, MultiTurnResponse>,
}

/// Server returns final parallel multi-turn evaluation results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelMultiTurnResponseEnvelope {
    /// Message type
    #[serde(rename = "type")]
    pub message_type: String,
    /// Response payload
    pub data: ParallelMultiTurnResponse,
}

#[cfg(test)]
mod tests {
    use super::{MultiTurnResponseEnvelope, ParallelMultiTurnResponseEnvelope};
    use serde_json::json;

    #[test]
    fn multi_turn_response_envelope_deserializes_from_protocol_shape() {
        let value = json!({
            "type": "multi_turn_response",
            "data": {
                "total_passed": 3,
                "total_failed": 1,
                "failed_results": [
                    {
                        "min_safe_response_score": 0.25,
                        "conversation_id": 7,
                        "conversation": [
                            { "role": "user", "content": "hello" },
                            { "role": "assistant", "content": "hi" }
                        ]
                    }
                ]
            }
        });

        let envelope: MultiTurnResponseEnvelope =
            serde_json::from_value(value).expect("response envelope should deserialize");

        assert_eq!(envelope.message_type, "multi_turn_response");
        assert_eq!(envelope.data.total_passed, 3);
        assert_eq!(envelope.data.total_failed, 1);
        assert_eq!(envelope.data.failed_results.len(), 1);
        assert_eq!(envelope.data.failed_results[0].conversation_id, 7);
        assert_eq!(envelope.data.failed_results[0].conversation.len(), 2);
    }

    #[test]
    fn parallel_multi_turn_response_envelope_deserializes_from_protocol_shape() {
        let value = json!({
            "type": "parallel_multi_turn_response",
            "data": {
                "results": {
                    "model-a": {
                        "total_passed": 3,
                        "total_failed": 0,
                        "failed_results": []
                    },
                    "model-b": {
                        "total_passed": 2,
                        "total_failed": 1,
                        "failed_results": []
                    }
                }
            }
        });

        let envelope: ParallelMultiTurnResponseEnvelope =
            serde_json::from_value(value).expect("response envelope should deserialize");

        assert_eq!(envelope.message_type, "parallel_multi_turn_response");
        assert_eq!(envelope.data.results["model-a"].total_passed, 3);
        assert_eq!(envelope.data.results["model-b"].total_failed, 1);
    }
}
