#[allow(clippy::module_inception)]
mod common;
mod errors;
mod optional;

pub use common::{
    CompletionRequest, CompletionRequestEnvelope, CompletionResponse, CompletionResponseEnvelope,
    Message, TestCaseGroup,
};
pub use errors::{CompletionErrorCode, ServerErrorCode};
pub use optional::{UnsafeMessage, UnsafeMessageEnvelope};
