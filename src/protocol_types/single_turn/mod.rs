use serde::{Deserialize, Serialize, de::Error};

mod optional;
mod request;
mod response;

pub use optional::{
    IterationComplete, IterationCompleteEnvelope, IterationStart, IterationStartEnvelope,
    OptionalSingleTurnMessage,
};
pub use request::{SingleTurnRequest, SingleTurnRequestEnvelope};
pub use response::{FailedSingleTurnResult, SingleTurnResponse, SingleTurnResponseEnvelope};

/// Messages that the server may send to the client during single-turn evaluation (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SingleTurnReceivableMessage {
    CompletionRequest(super::common::CompletionRequest),
    SingleTurnResponse(SingleTurnResponse),
    IterationStart(IterationStart),
    IterationComplete(IterationComplete),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CategorizedSingleTurnMessage {
    CompletionRequest(super::common::CompletionRequest),
    SingleTurnResponse(SingleTurnResponse),
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
            "iteration_complete" => Ok(SingleTurnReceivableMessage::IterationComplete(
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
        }
    }
}
