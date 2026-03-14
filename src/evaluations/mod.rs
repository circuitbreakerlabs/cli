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
    MultiTurn,
}

impl From<&crate::cli::EvaluationCommand> for EvaluationType {
    fn from(cmd: &crate::cli::EvaluationCommand) -> Self {
        match cmd {
            crate::cli::EvaluationCommand::SingleTurn { .. } => EvaluationType::SingleTurn,
            crate::cli::EvaluationCommand::MultiTurn { .. } => EvaluationType::MultiTurn,
        }
    }
}
