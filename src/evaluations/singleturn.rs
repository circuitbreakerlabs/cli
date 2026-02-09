use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use crate::protocol_types::single_turn::{
    SingleTurnReceivableMessage, SingleTurnRequest, SingleTurnRequestEnvelope,
};
use crate::protocol_types::{self};
use crate::websockets::{WebSocketClose, WebSocketConnection};

async fn handle_completion_request(
    request: protocol_types::CompletionRequest,
    completion_tx: tokio::sync::mpsc::Sender<
        Result<protocol_types::CompletionResponse, WebSocketClose>,
    >,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    todo!(
        "Implement logic to handle completion requests from the server, including generating responses and sending them back over the WebSocket connection"
    )
}

async fn reader_task(
    mut read: SplitStream<WebSocketConnection>,
    completion_tx: tokio::sync::mpsc::Sender<
        Result<protocol_types::CompletionResponse, WebSocketClose>,
    >,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                tracing::debug!("Received: {}", text);
                let msg = SingleTurnReceivableMessage::try_from(text.as_bytes());
                match msg {
                    Ok(SingleTurnReceivableMessage::CompletionRequest(req)) => {
                        tokio::spawn(handle_completion_request(req, completion_tx.clone()));
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
                tracing::error!("Couldn't receive message '{err}'");
                return Err(err.into());
            }
        }
    }
    Ok(())
}

async fn writer_task(
    mut write: SplitSink<WebSocketConnection, Message>,
    mut response_rx: tokio::sync::mpsc::Receiver<
        Result<protocol_types::CompletionResponse, WebSocketClose>,
    >,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    todo!(
        "Implement logic to listen for completion responses from the reader task and send them back to the server over the WebSocket connection"
    )
}

pub async fn run_single_turn_evaluation(
    websocket_connection: WebSocketConnection,
    request: SingleTurnRequest,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (mut write, mut read) = websocket_connection.split();
    let (mut completion_tx, mut completion_rx) = tokio::sync::mpsc::channel::<
        Result<protocol_types::CompletionResponse, WebSocketClose>,
    >(100);

    write
        .send(Message::Text(
            serde_json::to_string(&SingleTurnRequestEnvelope::from(request))?.into(),
        ))
        .await?;

    let reader_handle = tokio::spawn(reader_task(read, completion_tx.clone()));
    let write_handle = tokio::spawn(writer_task(write, completion_rx));

    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    reader_handle.await??;
    drop(completion_tx);
    write_handle.await??;

    Ok(())
}
