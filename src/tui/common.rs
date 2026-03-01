#[derive(Clone, Debug)]
pub enum WaitingFor {
    Provider,
    #[allow(clippy::upper_case_acronyms)]
    API,
}

#[derive(Clone, Debug)]
pub enum ConversationStatus {
    Waiting(WaitingFor),
    Passed,
    Failed,
    Warning,
}
