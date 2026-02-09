use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use crate::protocol_types::single_turn::{
    SingleTurnReceivableMessage, SingleTurnRequest, SingleTurnRequestEnvelope,
};
use crate::protocol_types::{self};
use crate::websockets::WebSocketConnection;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;

async fn handle_completion_request(
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

/// Listens for incoming messages from the server, processes them, and sends completion responses or errors back to the writer task.
async fn reader_task(
    mut read: SplitStream<WebSocketConnection>,
    completion_url: String,
    completion_tx: tokio::sync::mpsc::Sender<
        Result<protocol_types::CompletionResponse, CloseFrame>,
    >,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                tracing::debug!("Received message: {}", text);
                let msg = SingleTurnReceivableMessage::try_from(text.as_bytes());
                match msg {
                    Ok(SingleTurnReceivableMessage::CompletionRequest(req)) => {
                        tokio::spawn(handle_completion_request(
                            req,
                            completion_url.clone(),
                            completion_tx.clone(),
                        ));
                    }
                    Ok(SingleTurnReceivableMessage::IterationStart(msg)) => {
                        tracing::info!(
                            "Starting iteration {} with {} test cases",
                            msg.iteration_number,
                            msg.total_test_cases
                        );
                    }
                    Ok(SingleTurnReceivableMessage::IterationComplete(msg)) => {
                        tracing::info!(
                            "Completed iteration {}: {} passed, {} failed",
                            msg.iteration_number,
                            msg.total_passed,
                            msg.total_failed
                        );
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse incoming message '{e}'");
                        completion_tx
                            .send(Err(CloseFrame {
                                code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Protocol,
                                reason: format!("Failed to parse incoming message: {e}").into(),
                            }))
                            .await?;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                tracing::info!("Server closed connection");
                break;
            }
            Ok(msg) => {
                tracing::warn!("Received unexpected message type with value '{msg}'");
            }
            Err(err) => {
                let err = format!("Couldn't receive message '{err}'");
                tracing::error!("{}", &err);
                completion_tx
                    .send(Err(CloseFrame {
                        code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Error,
                        reason: err.clone().into(),
                    }))
                    .await?;
                return Err(err.into());
            }
        }
    }
    Ok(())
}

/// Listens for completion responses and errors from the reader task and forwards them to the server. If an error is received, it sends a close frame and terminates.
async fn writer_task(
    mut write: SplitSink<WebSocketConnection, Message>,
    mut completion_rx: tokio::sync::mpsc::Receiver<
        Result<protocol_types::CompletionResponse, CloseFrame>,
    >,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    while let Some(msg) = completion_rx.recv().await {
        match msg {
            Ok(completion_response) => {
                let msg = serde_json::to_string(&completion_response)?;
                write.send(Message::Text(msg.into())).await?;
            }
            Err(close_frame) => {
                let msg = Message::Close(Some(close_frame));
                write.send(msg).await?;
                break;
            }
        }
    }
    Ok(())
}

pub async fn run_single_turn_evaluation(
    websocket_connection: WebSocketConnection,
    request: SingleTurnRequest,
    completion_url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (mut write, read) = websocket_connection.split();
    let (completion_tx, completion_rx) =
        tokio::sync::mpsc::channel::<Result<protocol_types::CompletionResponse, CloseFrame>>(100);

    write
        .send(Message::Text(
            serde_json::to_string(&SingleTurnRequestEnvelope::from(request))?.into(),
        ))
        .await?;

    let reader_handle = tokio::spawn(reader_task(
        read,
        completion_url.to_string(),
        completion_tx.clone(),
    ));
    let write_handle = tokio::spawn(writer_task(write, completion_rx));

    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    reader_handle.await??;
    drop(completion_tx);
    write_handle.await??;

    Ok(())
}
