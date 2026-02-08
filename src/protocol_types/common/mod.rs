#[allow(clippy::module_inception)]
mod common;
mod errors;
mod optional;

pub use common::{
    CompletionRequest, CompletionRequestEnvelope, CompletionResponse, CompletionResponseEnvelope,
    Message, TestCaseGroup,
};
pub use errors::{
    CompletionError, CompletionErrorCode, CompletionErrorEnvelope, ServerError, ServerErrorCode,
    ServerErrorEnvelope,
};
pub use optional::{UnsafeMessage, UnsafeMessageEnvelope};
