#[allow(clippy::module_inception)]
mod common;
mod errors;
mod optional;

pub use common::{
    CompletionRequest, CompletionResponse, CompletionResponseEnvelope, ConversationId, Message,
    Role, TestCaseGroup,
};
pub use errors::{CompletionError, CompletionErrorEnvelope, ServerErrorCode};
pub use optional::{ConversationComplete, ConversationError};
