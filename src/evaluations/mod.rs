pub mod multiturn;
pub mod singleturn;

use crate::protocol_types::{self};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;

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

enum WriterMessage {
    CompletionResponse(protocol_types::CompletionResponse),
    Pong(Vec<u8>),
    Close(CloseFrame),
    ServerClosed,
}
