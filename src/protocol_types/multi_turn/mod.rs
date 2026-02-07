mod optional;
mod request;
mod response;

pub use optional::ConversationComplete;
pub use request::MultiTurnRequest;
pub use response::{FailedMultiTurnResult, MultiTurnResponse};
