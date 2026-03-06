use crate::response_provider::err::ProviderError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EvaluationError {
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Channel send error")]
    ChannelSend,

    #[error("Provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("WebSocket closed unexpectedly: {0}")]
    WebSocketClosed(String),

    #[error("Task join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for EvaluationError {
    fn from(_e: tokio::sync::mpsc::error::SendError<T>) -> Self {
        EvaluationError::ChannelSend
    }
}
