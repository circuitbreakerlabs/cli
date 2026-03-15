mod optional;
mod request;
mod response;

use crate::protocol_types::common::{CompletionRequest, ConversationComplete, ConversationError};
pub use optional::{MultiTurnEvaluationStart, OptionalMultiTurnMessage};
#[allow(unused_imports)]
pub use request::MultiTurnTestType;
pub use request::{MultiTurnRequest, MultiTurnRequestEnvelope};
pub use response::MultiTurnResponse;
use serde::{Deserialize, Serialize, de::Error};

/// Messages that the server may send to the client during multi-turn evaluation (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MultiTurnReceivableMessage {
    CompletionRequest(super::common::CompletionRequest),
    MultiTurnResponse(MultiTurnResponse),
    ConversationComplete(ConversationComplete),
    MultiTurnEvaluationStart(MultiTurnEvaluationStart),
    ConversationError(ConversationError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CategorizedMultiTurnMessage {
    CompletionRequest(CompletionRequest),
    MultiTurnResponse(MultiTurnResponse),
    OptionalMultiTurnMessage(OptionalMultiTurnMessage),
}

impl TryFrom<&[u8]> for MultiTurnReceivableMessage {
    type Error = serde_json::Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let json_str =
            std::str::from_utf8(bytes).map_err(|e| serde_json::Error::custom(e.to_string()))?;
        let base_message: serde_json::Value = serde_json::from_str(json_str)?;
        let message_type = base_message
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| serde_json::Error::custom("Missing 'type' field"))?;
        let data = base_message
            .get("data")
            .ok_or_else(|| serde_json::Error::custom("Missing 'data' field"))?
            .to_owned();

        match message_type {
            "completion_request" => Ok(MultiTurnReceivableMessage::CompletionRequest(
                serde_json::from_value(data)?,
            )),
            "multi_turn_response" => Ok(MultiTurnReceivableMessage::MultiTurnResponse(
                serde_json::from_value(data)?,
            )),
            "conversation_complete" => Ok(MultiTurnReceivableMessage::ConversationComplete(
                serde_json::from_value(data)?,
            )),
            "multi_turn_evaluation_start" => Ok(
                MultiTurnReceivableMessage::MultiTurnEvaluationStart(serde_json::from_value(data)?),
            ),
            "conversation_error" => Ok(MultiTurnReceivableMessage::ConversationError(
                serde_json::from_value(data)?,
            )),
            _ => Err(serde_json::Error::custom(format!(
                "Unknown message type: {message_type}",
            ))),
        }
    }
}

impl From<MultiTurnReceivableMessage> for CategorizedMultiTurnMessage {
    fn from(message: MultiTurnReceivableMessage) -> Self {
        match message {
            MultiTurnReceivableMessage::CompletionRequest(req) => {
                CategorizedMultiTurnMessage::CompletionRequest(req)
            }
            MultiTurnReceivableMessage::MultiTurnResponse(resp) => {
                CategorizedMultiTurnMessage::MultiTurnResponse(resp)
            }
            MultiTurnReceivableMessage::ConversationComplete(complete) => {
                CategorizedMultiTurnMessage::OptionalMultiTurnMessage(
                    optional::OptionalMultiTurnMessage::ConversationComplete(complete),
                )
            }
            MultiTurnReceivableMessage::MultiTurnEvaluationStart(start) => {
                CategorizedMultiTurnMessage::OptionalMultiTurnMessage(
                    optional::OptionalMultiTurnMessage::MultiTurnEvaluationStart(start),
                )
            }
            MultiTurnReceivableMessage::ConversationError(error) => {
                CategorizedMultiTurnMessage::OptionalMultiTurnMessage(
                    optional::OptionalMultiTurnMessage::ConversationError(error),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CategorizedMultiTurnMessage, MultiTurnReceivableMessage, OptionalMultiTurnMessage,
    };
    use serde_json::json;

    #[test]
    fn parses_multi_turn_response_message() {
        let payload = json!({
            "type": "multi_turn_response",
            "data": {
                "total_passed": 2,
                "total_failed": 1,
                "failed_results": [
                    {
                        "min_safe_response_score": 0.12,
                        "conversation_id": 9,
                        "conversation": [
                            { "role": "user", "content": "hello" },
                            { "role": "assistant", "content": "hi" }
                        ]
                    }
                ]
            }
        });

        let message = MultiTurnReceivableMessage::try_from(payload.to_string().as_bytes())
            .expect("message should parse");

        match message {
            MultiTurnReceivableMessage::MultiTurnResponse(resp) => {
                assert_eq!(resp.total_passed, 2);
                assert_eq!(resp.total_failed, 1);
                assert_eq!(resp.failed_results.len(), 1);
            }
            other => panic!("expected multi turn response, got {other:?}"),
        }
    }

    #[test]
    fn categorizes_evaluation_start_messages() {
        let payload = json!({
            "type": "multi_turn_evaluation_start",
            "data": {
                "conversation_ids": [10, 11]
            }
        });

        let message = MultiTurnReceivableMessage::try_from(payload.to_string().as_bytes())
            .expect("message should parse");
        let categorized = CategorizedMultiTurnMessage::from(message);

        match categorized {
            CategorizedMultiTurnMessage::OptionalMultiTurnMessage(
                OptionalMultiTurnMessage::MultiTurnEvaluationStart(start),
            ) => {
                assert_eq!(start.conversation_ids, vec![10, 11]);
            }
            other => panic!("expected evaluation start, got {other:?}"),
        }
    }

    #[test]
    fn rejects_message_without_data() {
        let payload = json!({
            "type": "conversation_complete"
        });

        let err = MultiTurnReceivableMessage::try_from(payload.to_string().as_bytes())
            .expect_err("missing data should fail");

        assert!(err.to_string().contains("Missing 'data' field"));
    }
}
