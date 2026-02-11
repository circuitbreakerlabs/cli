mod optional;
mod request;
mod response;

pub use optional::{ConversationComplete, ConversationCompleteEnvelope, MultiTurnEvaluationStart};
pub use request::{MultiTurnRequest, MultiTurnRequestEnvelope};
pub use response::{FailedMultiTurnResult, MultiTurnResponse, MultiTurnResponseEnvelope};
use serde::{Deserialize, Serialize};

/// Messages that the server may send to the client during multi-turn evaluation (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MultiTurnReceivableMessage {
    CompletionRequest(super::common::CompletionRequest),
    MultiTurnResponse(MultiTurnResponse),
    ConversationComplete(ConversationComplete),
    MultiTurnEvaluationStart(MultiTurnEvaluationStart),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CategorizedMultiTurnMessage {
    CompletionRequest(super::common::CompletionRequest),
    MultiTurnResponse(MultiTurnResponse),
    OptionalMultiTurnMessage(optional::OptionalMultiTurnMessage),
}
