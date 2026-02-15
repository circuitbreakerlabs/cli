use crate::protocol_types::common::CompletionResponseEnvelope;
use crate::protocol_types::multi_turn::{
    CategorizedMultiTurnMessage, MultiTurnReceivableMessage, MultiTurnRequest,
    MultiTurnRequestEnvelope, MultiTurnResponse,
};
use crate::protocol_types::{self};
use crate::response_provider::ResponseProvider;
use crate::websockets::WebSocketConnection;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use protocol_types::multi_turn::OptionalMultiTurnMessage;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;

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
    let completion = provider
        .generate_response(&request.messages)
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
    message: protocol_types::multi_turn::OptionalMultiTurnMessage,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match message {
        OptionalMultiTurnMessage::MultiTurnEvaluationStart(evaluation_start) => {
            tracing::info!(
                "Received MultiTurnEvaluationStart message: conversation_ids={:?}",
                evaluation_start.conversation_ids,
            );
        }
        OptionalMultiTurnMessage::ConversationComplete(conversation_complete) => {
            tracing::info!(
                "Received ConversationComplete message: conversation_id={}, turns={}, passed={}",
                conversation_complete.conversation_id,
                conversation_complete.turns,
                conversation_complete.passed,
            );
        }
        OptionalMultiTurnMessage::ConversationError(conversation_error) => {
            tracing::error!(
                "Received ConversationError message: conversation_id={}, error_message={}",
                conversation_error.conversation_id,
                conversation_error.error_message,
            );
        }
    }

    Ok(())
}

/// Listens for incoming messages from the server, processes them, and sends completion responses or errors back to the writer task.
async fn reader_task(
    mut read: SplitStream<WebSocketConnection>,
    provider: Arc<dyn ResponseProvider>,
    writer_tx: tokio::sync::mpsc::Sender<WriterMessage>,
) -> Result<MultiTurnResponse, Box<dyn std::error::Error + Send + Sync>> {
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                tracing::debug!("Received message: {}", text);
                let msg = MultiTurnReceivableMessage::try_from(text.as_bytes())
                    .map(CategorizedMultiTurnMessage::from);
                match msg {
                    Ok(CategorizedMultiTurnMessage::CompletionRequest(req)) => {
                        tokio::spawn(handle_completion_request(
                            req,
                            provider.clone(),
                            writer_tx.clone(),
                        ));
                    }
                    Ok(CategorizedMultiTurnMessage::MultiTurnResponse(resp)) => {
                        tracing::debug!(
                            "Received MultiTurnResponse, sending to writer task and terminating reader"
                        );
                        writer_tx.send(WriterMessage::ServerClosed).await?;
                        return Ok(resp);
                    }
                    Ok(CategorizedMultiTurnMessage::OptionalMultiTurnMessage(optional_msg)) => {
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
            Ok(Message::Close(frame)) => {
                if let Some(frame) = &frame {
                    tracing::debug!(
                        "Received close message from server with code {} and reason '{}'",
                        frame.code,
                        frame.reason
                    );
                } else {
                    tracing::debug!("Received close message from server without close frame");
                }
                writer_tx.send(WriterMessage::ServerClosed).await?;
                break;
            }
            Ok(Message::Ping(payload)) => {
                tracing::debug!("Received ping from server, sending pong");
                writer_tx
                    .send(WriterMessage::Pong(payload.to_vec()))
                    .await?;
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
    Err("WebSocket stream ended without receiving a MultiTurnResponse".into())
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
            WriterMessage::Pong(payload) => {
                write.send(Message::Pong(payload.into())).await?;
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
    provider: Arc<dyn ResponseProvider>,
    request: MultiTurnRequest,
) -> Result<MultiTurnResponse, Box<dyn std::error::Error + Send + Sync>> {
    let (mut write, read) = websocket_connection.split();
    let (writer_tx, writer_rx) = tokio::sync::mpsc::channel::<WriterMessage>(100);

    write
        .send(Message::Text(
            serde_json::to_string(&MultiTurnRequestEnvelope::from(request))?.into(),
        ))
        .await?;

    let reader_handle = tokio::spawn(reader_task(read, provider, writer_tx.clone()));
    let write_handle = tokio::spawn(writer_task(write, writer_rx));

    let multi_turn_response = reader_handle.await??;
    write_handle.await??;

    Ok(multi_turn_response)
}
