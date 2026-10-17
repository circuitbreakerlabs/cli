#![allow(dead_code)]

pub mod common;
pub mod multi_turn;
pub mod single_turn;

#[cfg(any(text_targets, test))]
pub use common::Role;
pub use common::{CompletionError, CompletionRequest, CompletionResponse, ConversationId, Message};
pub use multi_turn::MultiTurnEvaluationRequest;
#[cfg(eval_targets)]
pub use multi_turn::{MultiTurnEvalRequest, MultiTurnRerunEvalRequest};
#[cfg(eval_targets)]
pub use single_turn::SingleTurnEvalRequest;
pub use single_turn::SingleTurnEvaluationRequest;
#[cfg(eval_targets)]
pub use single_turn::SingleTurnRerunEvalRequest;
