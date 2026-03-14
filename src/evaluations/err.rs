use crate::protocol_types::common::ServerErrorCode;
use crate::response_provider::err::ProviderError;
use thiserror::Error;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;

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

    #[error("Server closed websocket with error code {code:?}: {reason}")]
    ServerClose {
        code: ServerErrorCode,
        reason: String,
    },

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

impl EvaluationError {
    pub(crate) fn from_close_frame_or_eof(
        frame: Option<&CloseFrame>,
        expected_response: &'static str,
    ) -> Self {
        if let Some(frame) = frame
            && let Ok(code) = ServerErrorCode::try_from(u16::from(frame.code))
        {
            return EvaluationError::ServerClose {
                code,
                reason: frame.reason.to_string(),
            };
        }

        EvaluationError::WebSocketClosed(format!(
            "WebSocket stream ended without receiving a {expected_response}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::EvaluationError;
    use crate::protocol_types::common::ServerErrorCode;
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

    #[test]
    fn maps_known_server_close_frames_to_typed_errors() {
        let frame = CloseFrame {
            code: CloseCode::Library(4001),
            reason: "bad token".into(),
        };

        let error = EvaluationError::from_close_frame_or_eof(Some(&frame), "SingleTurnResponse");

        assert!(matches!(
            error,
            EvaluationError::ServerClose {
                code: ServerErrorCode::Unauthorized,
                reason
            } if reason == "bad token"
        ));
    }

    #[test]
    fn keeps_generic_error_for_non_server_close_frames() {
        let frame = CloseFrame {
            code: CloseCode::Normal,
            reason: "bye".into(),
        };

        let error = EvaluationError::from_close_frame_or_eof(Some(&frame), "MultiTurnResponse");

        assert!(matches!(
            error,
            EvaluationError::WebSocketClosed(message)
                if message == "WebSocket stream ended without receiving a MultiTurnResponse"
        ));
    }

    #[test]
    fn keeps_generic_error_for_eof_without_close_frame() {
        let error = EvaluationError::from_close_frame_or_eof(None, "SingleTurnResponse");

        assert!(matches!(
            error,
            EvaluationError::WebSocketClosed(message)
                if message == "WebSocket stream ended without receiving a SingleTurnResponse"
        ));
    }
}
