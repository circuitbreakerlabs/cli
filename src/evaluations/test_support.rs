use crate::protocol_types::{self, Role};
use crate::response_provider::{ProviderError, ResponseProvider};
use crate::websockets::WebSocketConnection;

use async_trait::async_trait;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::future::Future;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{WebSocketStream, accept_async, connect_async};

pub(super) enum ProviderBehavior {
    Immediate(Result<String, ProviderError>),
    Gate(oneshot::Receiver<Result<String, ProviderError>>),
}

pub(super) struct ControlledProvider {
    behaviors: Mutex<HashMap<String, ProviderBehavior>>,
}

impl ControlledProvider {
    pub(super) fn new(behaviors: HashMap<String, ProviderBehavior>) -> Self {
        Self {
            behaviors: Mutex::new(behaviors),
        }
    }
}

#[async_trait]
impl ResponseProvider for ControlledProvider {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<protocol_types::Message, ProviderError> {
        let key = conversation_history
            .first()
            .expect("test conversations should have at least one message")
            .content
            .clone();
        let behavior = self
            .behaviors
            .lock()
            .await
            .remove(&key)
            .unwrap_or_else(|| panic!("missing provider behavior for key '{key}'"));

        let content = match behavior {
            ProviderBehavior::Immediate(result) => result,
            ProviderBehavior::Gate(receiver) => receiver
                .await
                .expect("provider gate sender should not be dropped"),
        }?;

        Ok(protocol_types::Message {
            role: Role::Assistant,
            content,
        })
    }
}

pub(super) fn gated_behavior() -> (
    ProviderBehavior,
    oneshot::Sender<Result<String, ProviderError>>,
) {
    let (sender, receiver) = oneshot::channel();
    (ProviderBehavior::Gate(receiver), sender)
}

pub(super) async fn spawn_websocket_server<F, Fut>(
    handler: F,
) -> (WebSocketConnection, tokio::task::JoinHandle<()>)
where
    F: FnOnce(WebSocketStream<TcpStream>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("server should bind");
    let addr = listener
        .local_addr()
        .expect("server should have a local address");

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("server should accept");
        let websocket = accept_async(stream)
            .await
            .expect("server websocket handshake should succeed");
        handler(websocket).await;
    });

    let (client, _) = connect_async(format!("ws://{addr}"))
        .await
        .expect("client websocket connection should succeed");

    (client, server)
}

pub(super) async fn recv_text_json(
    read: &mut SplitStream<WebSocketStream<TcpStream>>,
) -> serde_json::Value {
    match read.next().await.expect("websocket should yield a message") {
        Ok(Message::Text(text)) => {
            serde_json::from_str(&text).expect("text message should contain valid json")
        }
        other => panic!("expected text websocket message, got {other:?}"),
    }
}

pub(super) async fn send_json(
    write: &mut SplitSink<WebSocketStream<TcpStream>, Message>,
    value: serde_json::Value,
) {
    write
        .send(Message::Text(value.to_string().into()))
        .await
        .expect("server should send json text");
}
