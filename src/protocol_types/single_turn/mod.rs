use serde::{Deserialize, Serialize};

mod optional;
mod request;
mod response;

pub use optional::{
    IterationComplete, IterationCompleteEnvelope, IterationStart, IterationStartEnvelope,
};
pub use request::{SingleTurnRequest, SingleTurnRequestEnvelope};
pub use response::{FailedSingleTurnResult, SingleTurnResponse, SingleTurnResponseEnvelope};

/// Messages that the server may send to the client during single-turn evaluation (Server -> Client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SingleTurnReceivableMessage {
    CompletionRequest(super::common::CompletionRequest),
    IterationStart(IterationStart),
    IterationComplete(IterationComplete),
}
