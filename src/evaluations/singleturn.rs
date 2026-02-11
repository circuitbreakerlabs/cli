use crate::completions::CompletionGenerator;
use crate::protocol_types::common::CompletionResponseEnvelope;
use crate::protocol_types::single_turn::{
    CategorizedSingleTurnMessage, SingleTurnReceivableMessage, SingleTurnRequest,
    SingleTurnRequestEnvelope, SingleTurnResponse,
};
use crate::protocol_types::{self};
use crate::websockets::WebSocketConnection;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;

enum WriterMessage {
    CompletionResponse(protocol_types::CompletionResponse),
    Close(CloseFrame),
    ServerClosed,
}

async fn handle_completion_request(
    request: protocol_types::CompletionRequest,
    completion_generator: CompletionGenerator,
    writer_tx: tokio::sync::mpsc::Sender<WriterMessage>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let completion = completion_generator
        .generate_completions(&request.messages)
        .await
        .map_err(|e| e.to_string())?;

    writer_tx
        .send(WriterMessage::CompletionResponse(
            protocol_types::CompletionResponse {
                request_id: request.request_id.clone(),
                model_response: completion.content,
            },
        ))
        .await?;
    Ok(())
}

async fn handle_optional_message(
    message: protocol_types::single_turn::OptionalSingleTurnMessage,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match message {
        protocol_types::single_turn::OptionalSingleTurnMessage::IterationStart(iteration_start) => {
            tracing::info!(
                "Received IterationStart message: iteration_number={}, total_test_cases={}",
                iteration_start.iteration_number,
                iteration_start.total_test_cases
            );
        }
        protocol_types::single_turn::OptionalSingleTurnMessage::IterationComplete(
            iteration_complete,
        ) => {
            tracing::info!(
                "Received IterationComplete message: iteration_number={}, total_passed={}, total_failed={}",
                iteration_complete.iteration_number,
                iteration_complete.total_passed,
                iteration_complete.total_failed
            );
        }
    }

    Ok(())
}

/// Listens for incoming messages from the server, processes them, and sends completion responses or errors back to the writer task.
async fn reader_task(
    mut read: SplitStream<WebSocketConnection>,
    completion_generator: CompletionGenerator,
    writer_tx: tokio::sync::mpsc::Sender<WriterMessage>,
) -> Result<SingleTurnResponse, Box<dyn std::error::Error + Send + Sync>> {
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                tracing::debug!("Received message: {}", text);
                let msg = SingleTurnReceivableMessage::try_from(text.as_bytes())
                    .map(CategorizedSingleTurnMessage::from);
                match msg {
                    Ok(CategorizedSingleTurnMessage::CompletionRequest(req)) => {
                        tokio::spawn(handle_completion_request(
                            req,
                            completion_generator.clone(),
                            writer_tx.clone(),
                        ));
                    }
                    Ok(CategorizedSingleTurnMessage::SingleTurnResponse(resp)) => {
                        tracing::debug!(
                            "Received SingleTurnResponse, sending to writer task and terminating reader"
                        );
                        writer_tx.send(WriterMessage::ServerClosed).await?;
                        return Ok(resp);
                    }
                    Ok(CategorizedSingleTurnMessage::OptionalSingleTurnMessage(optional_msg)) => {
                        tokio::spawn(handle_optional_message(optional_msg));
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse incoming message '{e}'");
                        writer_tx
                            .send(WriterMessage::Close(CloseFrame {
                                code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Protocol,
                                reason: format!("Failed to parse incoming message: {e}").into(),
                            }))
                            .await?;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                writer_tx.send(WriterMessage::ServerClosed).await?;
                tracing::debug!("Reader task received close message, terminating");
                break;
            }
            Ok(msg) => {
                tracing::warn!("Received unexpected message type with value '{msg}'");
            }
            Err(err) => {
                let err = format!("Couldn't receive message '{err}'");
                tracing::error!("{}", &err);
                writer_tx
                    .send(WriterMessage::Close(CloseFrame {
                        code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Error,
                        reason: err.clone().into(),
                    }))
                    .await?;
                return Err(err.into());
            }
        }
    }
    Err("WebSocket stream ended without receiving a SingleTurnResponse".into())
}

/// Listens for completion responses and errors from the reader task and forwards them to the server. If an error is received, it sends a close frame and terminates.
async fn writer_task(
    mut write: SplitSink<WebSocketConnection, Message>,
    mut writer_rx: tokio::sync::mpsc::Receiver<WriterMessage>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    while let Some(msg) = writer_rx.recv().await {
        match msg {
            WriterMessage::CompletionResponse(completion_response) => {
                let msg =
                    serde_json::to_string(&CompletionResponseEnvelope::from(completion_response))?;
                write.send(Message::Text(msg.into())).await?;
            }
            WriterMessage::Close(close_frame) => {
                let msg = Message::Close(Some(close_frame));
                write.send(msg).await?;
                break;
            }
            WriterMessage::ServerClosed => {
                tracing::debug!("Server closed connection, writer task terminating");
                break;
            }
        }
    }
    Ok(())
}

pub async fn run_evaluation(
    websocket_connection: WebSocketConnection,
    completion_generator: CompletionGenerator,
    request: SingleTurnRequest,
) -> Result<SingleTurnResponse, Box<dyn std::error::Error + Send + Sync>> {
    let (mut write, read) = websocket_connection.split();
    let (writer_tx, writer_rx) = tokio::sync::mpsc::channel::<WriterMessage>(100);

    write
        .send(Message::Text(
            serde_json::to_string(&SingleTurnRequestEnvelope::from(request))?.into(),
        ))
        .await?;

    let reader_handle = tokio::spawn(reader_task(read, completion_generator, writer_tx.clone()));
    let write_handle = tokio::spawn(writer_task(write, writer_rx));

    let single_turn_response = reader_handle.await??;
    write_handle.await??;

    Ok(single_turn_response)
}
