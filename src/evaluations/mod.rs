mod engine;
pub mod err;
pub mod multiturn;
pub mod singleturn;
#[cfg(test)]
mod test_support;

pub use err::EvaluationError;

#[derive(Clone, Debug)]
pub enum EvaluationType {
    SingleTurn,
    SingleTurnRerun,
    MultiTurn,
    MultiTurnRerun,
}

impl From<&crate::cli::EvaluationCommand> for EvaluationType {
    fn from(cmd: &crate::cli::EvaluationCommand) -> Self {
        match cmd {
            crate::cli::EvaluationCommand::SingleTurn { .. } => EvaluationType::SingleTurn,
            crate::cli::EvaluationCommand::MultiTurn { .. } => EvaluationType::MultiTurn,
            crate::cli::EvaluationCommand::ReRun { rerun } => match rerun {
                crate::cli::ReRunEvaluationCommand::SingleTurn { .. } => {
                    EvaluationType::SingleTurnRerun
                }
                crate::cli::ReRunEvaluationCommand::MultiTurn { .. } => {
                    EvaluationType::MultiTurnRerun
                }
            },
        }
    }
}
