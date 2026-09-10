mod engine;
pub mod err;
pub mod multiturn;
pub mod singleturn;
#[cfg(test)]
mod test_support;

pub use err::EvaluationError;

#[derive(Clone, Debug)]
pub enum EvaluationType {
    Voice,
    VoiceRerun,
    SingleTurnVoice,
    SingleTurn,
    SingleTurnRerun,
    MultiTurn,
    MultiTurnRerun,
}

impl From<&crate::cli::EvaluationCommand> for EvaluationType {
    fn from(cmd: &crate::cli::EvaluationCommand) -> Self {
        match cmd {
            crate::cli::EvaluationCommand::SingleTurn { voice, .. } => {
                if *voice {
                    EvaluationType::SingleTurnVoice
                } else {
                    EvaluationType::SingleTurn
                }
            }
            crate::cli::EvaluationCommand::MultiTurn { voice, .. } => {
                if *voice {
                    EvaluationType::Voice
                } else {
                    EvaluationType::MultiTurn
                }
            }
            crate::cli::EvaluationCommand::ReRun { rerun } => match rerun {
                crate::cli::ReRunEvaluationCommand::SingleTurn { .. } => {
                    EvaluationType::SingleTurnRerun
                }
                crate::cli::ReRunEvaluationCommand::MultiTurn { voice, .. } => {
                    if *voice {
                        EvaluationType::VoiceRerun
                    } else {
                        EvaluationType::MultiTurnRerun
                    }
                }
            },
        }
    }
}
