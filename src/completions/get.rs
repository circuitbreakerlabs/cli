use crate::protocol_types::{self};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;

pub async fn get(
    request: protocol_types::CompletionRequest,
    completion_url: String,
    completion_tx: tokio::sync::mpsc::Sender<
        Result<protocol_types::CompletionResponse, CloseFrame>,
    >,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mock_completion = protocol_types::CompletionResponse {
        request_id: request.request_id.clone(),
        model_response: "This is a mock completion response".to_string(),
    };
    tracing::warn!(
        "Received completion request with id '{}', sending back mock response",
        request.request_id
    );
    completion_tx.send(Ok(mock_completion)).await?;
    Ok(())
}
