use crate::protocol_types::common::{CompletionErrorEnvelope, CompletionResponseEnvelope};
use crate::protocol_types::single_turn::{
    CategorizedSingleTurnMessage, SingleTurnReceivableMessage, SingleTurnRequest,
    SingleTurnRequestEnvelope, SingleTurnResponse,
};
use crate::protocol_types::{self};
use crate::response_provider::ResponseProvider;
use crate::tui::SingleTurnProgressIndicatorMessage;
use crate::tui::WaitingFor;
use crate::websockets::WebSocketConnection;

use super::WriterMessage;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use protocol_types::single_turn::OptionalSingleTurnMessage;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;

async fn handle_completion_request(
    request: protocol_types::CompletionRequest,
    provider: Arc<dyn ResponseProvider>,
    writer_tx: tokio::sync::mpsc::Sender<WriterMessage>,
    progress_indicator: Option<tokio::sync::mpsc::Sender<SingleTurnProgressIndicatorMessage>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(progress_indicator) = &progress_indicator {
        progress_indicator
            .send(SingleTurnProgressIndicatorMessage::WaitingFor {
                conversation_id: request.conversation_id,
                waiting_for: WaitingFor::Provider,
            })
            .await?;
    }

    let msg = match provider.generate_response(&request.messages).await {
        Ok(completion) => WriterMessage::CompletionResponse(protocol_types::CompletionResponse {
            request_id: request.request_id.clone(),
            model_response: completion.content,
        }),
        Err(e) => {
            tracing::error!("Error generating response: {e}");
            writer_tx
                .send(WriterMessage::CompletionError(
                    protocol_types::CompletionError {
                        request_id: request.request_id.clone(),
                        error_reason: (&e).into(),
                    },
                ))
                .await?;

            return Err(e.into());
        }
    };

    if let Some(progress_indicator) = progress_indicator {
        progress_indicator
            .send(SingleTurnProgressIndicatorMessage::WaitingFor {
                conversation_id: request.conversation_id,
                waiting_for: WaitingFor::API,
            })
            .await?;
    }

    writer_tx.send(msg).await?;
    Ok(())
}

async fn handle_optional_message(
    message: protocol_types::single_turn::OptionalSingleTurnMessage,
    progress_indicator: Option<tokio::sync::mpsc::Sender<SingleTurnProgressIndicatorMessage>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match message {
        OptionalSingleTurnMessage::IterationStart(iteration_start) => {
            tracing::info!(
                "Received IterationStart message: iteration_number={}, conversation_ids={:?}",
                iteration_start.iteration_number,
                iteration_start.conversation_ids,
            );
            if let Some(progress_indicator) = progress_indicator {
                progress_indicator
                    .send(SingleTurnProgressIndicatorMessage::IterationStart(
                        iteration_start,
                    ))
                    .await?;
            }
        }
        OptionalSingleTurnMessage::IterationComplete(iteration_complete) => {
            tracing::info!(
                "Received IterationComplete message: iteration_number={}, passed_conversation_ids={:?}, failed_conversation_ids={:?}",
                iteration_complete.iteration_number,
                iteration_complete.passed_conversation_ids,
                iteration_complete.failed_conversation_ids
            );
            if let Some(progress_indicator) = progress_indicator {
                progress_indicator
                    .send(SingleTurnProgressIndicatorMessage::IterationComplete(
                        iteration_complete,
                    ))
                    .await?;
            }
        }
        OptionalSingleTurnMessage::ConversationError(conversation_error) => {
            tracing::error!(
                "Received ConversationError message: conversation_id={}, error_message={}",
                conversation_error.conversation_id,
                conversation_error.error_message
            );
            if let Some(progress_indicator) = progress_indicator {
                progress_indicator
                    .send(SingleTurnProgressIndicatorMessage::ConversationError(
                        conversation_error,
                    ))
                    .await?;
            }
        }
        OptionalSingleTurnMessage::ConversationComplete(conversation_complete) => {
            tracing::info!(
                "Received ConversationComplete message: conversation_id={}, passed={}",
                conversation_complete.conversation_id,
                conversation_complete.passed
            );
            if let Some(progress_indicator) = progress_indicator {
                progress_indicator
                    .send(SingleTurnProgressIndicatorMessage::ConversationComplete(
                        conversation_complete,
                    ))
                    .await?;
            }
        }
    }

    Ok(())
}

/// Listens for incoming messages from the server, processes them, and sends completion responses or errors back to the writer task.
async fn reader_task(
    mut read: SplitStream<WebSocketConnection>,
    provider: Arc<dyn ResponseProvider>,
    writer_tx: tokio::sync::mpsc::Sender<WriterMessage>,
    progress_indicator: Option<tokio::sync::mpsc::Sender<SingleTurnProgressIndicatorMessage>>,
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
                            provider.clone(),
                            writer_tx.clone(),
                            progress_indicator.clone(),
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
                        tokio::spawn(handle_optional_message(
                            optional_msg,
                            progress_indicator.clone(),
                        ));
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
            WriterMessage::CompletionError(completion_error) => {
                let msg = serde_json::to_string(&CompletionErrorEnvelope::from(completion_error))?;
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
    request: SingleTurnRequest,
    progress_indicator: Option<tokio::sync::mpsc::Sender<SingleTurnProgressIndicatorMessage>>,
) -> Result<SingleTurnResponse, Box<dyn std::error::Error + Send + Sync>> {
    let (mut write, read) = websocket_connection.split();
    let (writer_tx, writer_rx) = tokio::sync::mpsc::channel::<WriterMessage>(100);

    write
        .send(Message::Text(
            serde_json::to_string(&SingleTurnRequestEnvelope::from(request))?.into(),
        ))
        .await?;

    let reader_handle = tokio::spawn(reader_task(
        read,
        provider,
        writer_tx.clone(),
        progress_indicator,
    ));
    let write_handle = tokio::spawn(writer_task(write, writer_rx));

    let single_turn_response = reader_handle.await??;
    write_handle.await??;

    Ok(single_turn_response)
}
