mod multiturn;
mod singleturn;
pub use multiturn::MultiTurnProgressIndicatorMessage;

pub enum WaitingFor {
    Provider,
    #[allow(clippy::upper_case_acronyms)]
    API,
}

pub enum ConversationStatus {
    Waiting(WaitingFor),
    Passed,
    Failed,
    Warning,
}
