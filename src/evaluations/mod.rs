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
            crate::cli::EvaluationCommand::SingleTurn { request, .. } => {
                if request.test_result_id.is_some() {
                    EvaluationType::SingleTurnRerun
                } else {
                    EvaluationType::SingleTurn
                }
            }
            crate::cli::EvaluationCommand::MultiTurn { request, .. } => {
                if request.test_result_id.is_some() {
                    EvaluationType::MultiTurnRerun
                } else {
                    EvaluationType::MultiTurn
                }
            }
        }
    }
}
