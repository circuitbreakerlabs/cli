#[allow(clippy::module_inception)]
mod common;
mod errors;
mod optional;

pub use common::{
    CompletionRequest, CompletionResponse, CompletionResponseEnvelope, Message, Role, TestCaseGroup,
};
pub use optional::{ConversationComplete, ConversationError};
