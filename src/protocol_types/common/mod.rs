#[allow(clippy::module_inception)]
mod common;
mod errors;
mod optional;

pub use common::{
    CompletionRequest, CompletionResponse, CompletionResponseEnvelope, ConversationId,
    EvaluationId, Message, RerunSelector, TestCaseGroup, TestResultIds, parse_evaluation_id,
    parse_test_case_group, parse_test_result_ids,
};
pub use errors::{
    CompletionError, CompletionErrorEnvelope, CompletionErrorReason, ServerErrorCode,
};
pub use optional::{ConversationComplete, ConversationError};

#[cfg(any(text_targets, test))]
pub use common::Role;
