use serde::{Deserialize, Serialize, de::Error};

mod optional;
mod request;
mod response;

use super::common::{CompletionRequest, ConversationComplete, ConversationError};
pub use optional::{IterationComplete, IterationStart, OptionalSingleTurnMessage};
pub use request::{SingleTurnRequest, SingleTurnRequestEnvelope};
pub use response::{ParallelSingleTurnResponse, SingleTurnResponse};

/// Messages that the server may send to the client during single-turn evaluation (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SingleTurnReceivableMessage {
    CompletionRequest(CompletionRequest),
    SingleTurnResponse(SingleTurnResponse),
    ParallelSingleTurnResponse(ParallelSingleTurnResponse),
    IterationStart(IterationStart),
    IterationComplete(IterationComplete),
    ConversationComplete(ConversationComplete),
    ConversationError(ConversationError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CategorizedSingleTurnMessage {
    CompletionRequest(super::common::CompletionRequest),
    SingleTurnResponse(SingleTurnResponse),
    ParallelSingleTurnResponse(ParallelSingleTurnResponse),
    OptionalSingleTurnMessage(OptionalSingleTurnMessage),
}

impl TryFrom<&[u8]> for SingleTurnReceivableMessage {
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
            "completion_request" => Ok(SingleTurnReceivableMessage::CompletionRequest(
                serde_json::from_value(data)?,
            )),
            "iteration_start" => Ok(SingleTurnReceivableMessage::IterationStart(
                serde_json::from_value(data)?,
            )),
            "single_turn_response" => Ok(SingleTurnReceivableMessage::SingleTurnResponse(
                serde_json::from_value(data)?,
            )),
            "parallel_single_turn_response" => {
                Ok(SingleTurnReceivableMessage::ParallelSingleTurnResponse(
                    serde_json::from_value(data)?,
                ))
            }
            "iteration_complete" => Ok(SingleTurnReceivableMessage::IterationComplete(
                serde_json::from_value(data)?,
            )),
            "conversation_complete" => Ok(SingleTurnReceivableMessage::ConversationComplete(
                serde_json::from_value(data)?,
            )),
            "conversation_error" => Ok(SingleTurnReceivableMessage::ConversationError(
                serde_json::from_value(data)?,
            )),
            _ => Err(serde_json::Error::custom(format!(
                "Unknown message type: {message_type}",
            ))),
        }
    }
}

impl From<SingleTurnReceivableMessage> for CategorizedSingleTurnMessage {
    fn from(message: SingleTurnReceivableMessage) -> Self {
        match message {
            SingleTurnReceivableMessage::CompletionRequest(req) => {
                CategorizedSingleTurnMessage::CompletionRequest(req)
            }
            SingleTurnReceivableMessage::SingleTurnResponse(resp) => {
                CategorizedSingleTurnMessage::SingleTurnResponse(resp)
            }
            SingleTurnReceivableMessage::ParallelSingleTurnResponse(resp) => {
                CategorizedSingleTurnMessage::ParallelSingleTurnResponse(resp)
            }
            SingleTurnReceivableMessage::IterationStart(start) => {
                CategorizedSingleTurnMessage::OptionalSingleTurnMessage(
                    OptionalSingleTurnMessage::IterationStart(start),
                )
            }
            SingleTurnReceivableMessage::IterationComplete(complete) => {
                CategorizedSingleTurnMessage::OptionalSingleTurnMessage(
                    OptionalSingleTurnMessage::IterationComplete(complete),
                )
            }
            SingleTurnReceivableMessage::ConversationComplete(complete) => {
                CategorizedSingleTurnMessage::OptionalSingleTurnMessage(
                    OptionalSingleTurnMessage::ConversationComplete(complete),
                )
            }
            SingleTurnReceivableMessage::ConversationError(error) => {
                CategorizedSingleTurnMessage::OptionalSingleTurnMessage(
                    OptionalSingleTurnMessage::ConversationError(error),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CategorizedSingleTurnMessage, OptionalSingleTurnMessage, SingleTurnReceivableMessage,
    };
    use serde_json::json;

    #[test]
    fn parses_completion_request_message() {
        let payload = json!({
            "type": "completion_request",
            "data": {
                "request_id": "req-1",
                "conversation_id": 7,
                "messages": [
                    { "role": "user", "content": "hello" }
                ]
            }
        });

        let message = SingleTurnReceivableMessage::try_from(payload.to_string().as_bytes())
            .expect("message should parse");

        match message {
            SingleTurnReceivableMessage::CompletionRequest(req) => {
                assert_eq!(req.request_id, "req-1");
                assert_eq!(req.conversation_id, 7);
                assert_eq!(req.messages.len(), 1);
            }
            other => panic!("expected completion request, got {other:?}"),
        }
    }

    #[test]
    fn parses_parallel_single_turn_response_message() {
        let payload = json!({
            "type": "parallel_single_turn_response",
            "data": {
                "results": {
                    "model-a": {
                        "total_passed": 1,
                        "total_failed": 0,
                        "failed_results": []
                    }
                }
            }
        });

        let message = SingleTurnReceivableMessage::try_from(payload.to_string().as_bytes())
            .expect("message should parse");

        match message {
            SingleTurnReceivableMessage::ParallelSingleTurnResponse(resp) => {
                assert_eq!(resp.results["model-a"].total_passed, 1);
            }
            other => panic!("expected parallel single turn response, got {other:?}"),
        }
    }

    #[test]
    fn categorizes_optional_messages() {
        let payload = json!({
            "type": "iteration_complete",
            "data": {
                "iteration_number": 2,
                "passed_conversation_ids": [1, 2],
                "failed_conversation_ids": [3]
            }
        });

        let message = SingleTurnReceivableMessage::try_from(payload.to_string().as_bytes())
            .expect("message should parse");
        let categorized = CategorizedSingleTurnMessage::from(message);

        match categorized {
            CategorizedSingleTurnMessage::OptionalSingleTurnMessage(
                OptionalSingleTurnMessage::IterationComplete(iteration),
            ) => {
                assert_eq!(iteration.iteration_number, 2);
                assert_eq!(iteration.passed_conversation_ids, vec![1, 2]);
                assert_eq!(iteration.failed_conversation_ids, vec![3]);
            }
            other => panic!("expected iteration complete, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_message_type() {
        let payload = json!({
            "type": "unexpected",
            "data": {}
        });

        let err = SingleTurnReceivableMessage::try_from(payload.to_string().as_bytes())
            .expect_err("unknown message type should fail");

        assert!(err.to_string().contains("Unknown message type"));
    }

    #[test]
    fn rejects_message_without_type() {
        let payload = json!({
            "data": {}
        });

        let err = SingleTurnReceivableMessage::try_from(payload.to_string().as_bytes())
            .expect_err("missing type should fail");

        assert!(err.to_string().contains("Missing 'type' field"));
    }
}
