use super::{
    Error, Result,
    config::Config,
    livekit::{Provider, VoiceSessionProvider, emit},
    protocol::{Audio, Command, Input, MAX_CONTROL_BYTES, QUEUE_FRAMES, VERSION},
};
use crate::{
    protocol_types::MultiTurnEvaluationRequest, tui::MultiTurnProgressIndicatorMessage,
    websockets::WebSocketConnection,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{collections::HashMap, path::Path, sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, watch},
    task::JoinSet,
};
use tokio_tungstenite::tungstenite::Message;

#[allow(clippy::too_many_lines)] // Central multiplexing loop and its cleanup share task ownership.
pub async fn run(
    websocket: WebSocketConnection,
    path: &Path,
    request: MultiTurnEvaluationRequest,
    progress: Option<mpsc::Sender<MultiTurnProgressIndicatorMessage>>,
) -> Result<Value> {
    let config = Config::load(path)?;
    // Resolve secrets and compile hooks before asking the API to start paid work.
    config.context(
        0,
        u32::try_from(request.max_turns()).map_err(|_| Error::Protocol)?,
    )?;
    super::hooks::Hooks::new(&config)?;
    if let Some(path) = &config.bootstrap_script {
        super::hooks::Script::load(path)?;
    } else {
        super::config::read_secret(
            config
                .token_env
                .as_deref()
                .ok_or(Error::Configuration("missing token_env"))?,
        )?;
    }
    let (kind, data) = match &request {
        MultiTurnEvaluationRequest::Standard(data) => ("voice_request", serde_json::to_value(data)),
        MultiTurnEvaluationRequest::Rerun(data) => {
            ("voice_rerun_request", serde_json::to_value(data))
        }
    };
    let target =
        json!({"label":config.label,"transport":"livekit","cli_version":env!("CARGO_PKG_VERSION")});
    let provider: Arc<dyn VoiceSessionProvider> = Arc::new(Provider(config));
    let (mut writer, mut reader) = websocket.split();
    writer.send(Message::Text(json!({"type":kind,"version":VERSION,"data":data.map_err(|_| Error::Protocol)?,"target":target}).to_string().into())).await.map_err(|_| Error::Transport)?;
    let (output, mut outgoing) = mpsc::channel::<Message>(QUEUE_FRAMES);
    let mut sessions = HashMap::<i32, (mpsc::Sender<Input>, watch::Sender<bool>)>::new();
    let mut tasks = JoinSet::new();
    let result = async {
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => return Err(Error::Cancelled),
                item = tasks.join_next(), if !tasks.is_empty() => {
                    if item.is_some_and(|result| result.is_err()) { return Err(Error::Transport); }
                }
                message = outgoing.recv() => {
                    let message = message.ok_or(Error::Transport)?;
                    if let Message::Text(text) = &message {
                        let event: Value = serde_json::from_str(text).map_err(|_| Error::Protocol)?;
                        if event["type"] == "response_end" {
                            let id = i32::try_from(event["session_id"].as_i64().ok_or(Error::Protocol)?).map_err(|_| Error::Protocol)?;
                            turn_progress(progress.as_ref(), id, false).await;
                        }
                    }
                    writer.send(message).await.map_err(|_| Error::Transport)?;
                }
                message = reader.next() => {
                    let message = message.ok_or(Error::Transport)?.map_err(|_| Error::Transport)?;
                    let input = match message {
                        Message::Ping(data) => { writer.send(Message::Pong(data)).await.map_err(|_| Error::Transport)?; continue; }
                        Message::Pong(_) => continue,
                        Message::Close(_) => return Err(Error::Transport),
                        Message::Binary(bytes) => Input::Audio(Audio::decode(&bytes)?),
                        Message::Text(text) => {
                            if text.len() > 16 * 1024 * 1024 { return Err(Error::Protocol); }
                            let value: Value = serde_json::from_str(&text).map_err(|_| Error::Protocol)?;
                            if text.len() > MAX_CONTROL_BYTES && value["type"] != "voice_result" { return Err(Error::Protocol); }
                            match value.get("type").and_then(Value::as_str) {
                                Some("voice_ready") => {
                                    if value["version"] != VERSION { return Err(Error::Protocol); }
                                    continue;
                                }
                                Some("voice_result") => {
                                    serde_json::from_value::<crate::protocol_types::multi_turn::MultiTurnResponse>(value["data"].clone()).map_err(|_| Error::Protocol)?;
                                    return Ok(value["data"].clone());
                                },
                                Some("voice_error") => return Err(Error::Protocol),
                                Some("multi_turn_evaluation_start" | "conversation_complete" | "conversation_error") => {
                                    if let Some(tx) = &progress { forward_progress(tx, &value, request.max_turns()).await?; }
                                    else { tracing::info!(event = value["type"].as_str().unwrap_or("progress"), "Voice evaluation progress"); }
                                    continue;
                                }
                                _ => Input::Control(serde_json::from_value(value).map_err(|_| Error::Protocol)?),
                            }
                        }
                        Message::Frame(_) => return Err(Error::Protocol),
                    };
                    if let Input::Control(Command::UtteranceStart { session_id, .. }) = &input {
                        turn_progress(progress.as_ref(), *session_id, true).await;
                    }
                    let id = match &input { Input::Audio(audio) => audio.header.session_id, Input::Control(command) => command.session_id() };
                    if let Input::Control(Command::SessionOpen { max_turns, timeout_ms, .. }) = input {
                        if sessions.contains_key(&id) || sessions.len() >= 128 || timeout_ms == 0 { return Err(Error::Protocol); }
                        let (tx, rx) = mpsc::channel(QUEUE_FRAMES + 2);
                        let (session_stop, stop_rx) = watch::channel(false);
                        sessions.insert(id, (tx, session_stop));
                        let provider = provider.clone();
                        let output = output.clone();
                        tasks.spawn(async move {
                            let execution = async {
                                let session = tokio::time::timeout(Duration::from_millis(timeout_ms.min(3_300_000)), provider.connect(id, max_turns)).await.map_err(|_| Error::Timeout)??;
                                session.run(rx, output.clone(), stop_rx).await
                            };
                            let result = execution.await;
                            if let Err(error) = result {
                                tracing::warn!(session_id = id, error = %error, "Voice session failed");
                                let code = match error { Error::Authentication => "authentication", Error::Timeout => "timeout", _ => "transport" };
                                let _ = emit(&output, json!({"type":"session_error","session_id":id,"code":code})).await;
                            }
                        });
                    } else if matches!(input, Input::Control(Command::SessionClose { .. } | Command::PlaybackCancel { .. })) {
                        if let Some((_, stop)) = sessions.remove(&id) { let _ = stop.send(true); }
                    } else {
                        let Some((tx, session_stop)) = sessions.get(&id) else { return Err(Error::Protocol); };
                        if tx.try_send(input).is_err() {
                            let _ = session_stop.send(true);
                            emit(&output, json!({"type":"session_error","session_id":id,"code":"overflow"})).await?;
                        }
                    }
                }
            }
        }
    }.await;
    for (_, session_stop) in sessions.values() {
        let _ = session_stop.send(true);
    }
    // Continue draining output while sessions close so cleanup cannot deadlock on
    // a full writer queue after the evaluator has returned its final result.
    let cleanup = async {
        while !tasks.is_empty() {
            tokio::select! { _ = tasks.join_next() => {}, _ = outgoing.recv() => {} }
        }
    };
    if tokio::time::timeout(Duration::from_secs(8), cleanup)
        .await
        .is_err()
    {
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    }
    let _ = writer.close().await;
    if let Some(tx) = progress {
        let _ = tx
            .send(MultiTurnProgressIndicatorMessage::EvaluationComplete)
            .await;
    }
    result
}

async fn forward_progress(
    tx: &mpsc::Sender<MultiTurnProgressIndicatorMessage>,
    value: &Value,
    max_turns: usize,
) -> Result<()> {
    let data = value["data"].clone();
    let progress = match value["type"].as_str() {
        Some("multi_turn_evaluation_start") => MultiTurnProgressIndicatorMessage::EvaluationStart {
            conversation_ids: serde_json::from_value(data["conversation_ids"].clone())
                .map_err(|_| Error::Protocol)?,
            max_turns,
        },
        Some("conversation_complete") => MultiTurnProgressIndicatorMessage::ConversationComplete(
            serde_json::from_value(data).map_err(|_| Error::Protocol)?,
        ),
        Some("conversation_error") => MultiTurnProgressIndicatorMessage::ConversationError(
            serde_json::from_value(data).map_err(|_| Error::Protocol)?,
        ),
        _ => return Ok(()),
    };
    let _ = tx.send(progress).await;
    Ok(())
}

async fn turn_progress(
    progress: Option<&mpsc::Sender<MultiTurnProgressIndicatorMessage>>,
    id: i32,
    customer: bool,
) {
    use crate::tui::WaitingFor;
    if let Some(tx) = progress {
        let _ = tx
            .send(MultiTurnProgressIndicatorMessage::ConversationTurn {
                conversation_id: id,
            })
            .await;
        let _ = tx
            .send(MultiTurnProgressIndicatorMessage::WaitingFor {
                conversation_id: id,
                waiting_for: if customer {
                    WaitingFor::Provider
                } else {
                    WaitingFor::API
                },
            })
            .await;
    }
}
