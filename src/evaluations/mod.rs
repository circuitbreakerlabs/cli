pub mod multiturn;
pub mod singleturn;

use crate::protocol_types::{self};
use crate::response_provider::ResponseProvider;

use std::sync::Arc;
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

async fn handle_completion_request(
    request: protocol_types::CompletionRequest,
    provider: Arc<dyn ResponseProvider>,
    writer_tx: tokio::sync::mpsc::Sender<WriterMessage>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let msg = match provider.generate_response(&request.messages).await {
        Ok(completion) => WriterMessage::CompletionResponse(protocol_types::CompletionResponse {
            request_id: request.request_id.clone(),
            model_response: completion.content,
        }),
        Err(e) => {
            let err = format!("Error generating response: {e}");
            tracing::error!("{}", &err);
            return Err(err.into());
        }
    };

    writer_tx.send(msg).await?;
    Ok(())
}
