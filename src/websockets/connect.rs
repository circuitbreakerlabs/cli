use crate::consts::headers;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::info;

pub type WebSocketConnection = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub async fn connect(url: &str, api_key: &str) -> Result<WebSocketConnection, tungstenite::Error> {
    let mut request = url.into_client_request()?;
    request
        .headers_mut()
        .insert(headers::CBL_API_KEY, api_key.parse()?);

    let (ws_stream, _resp) = tokio_tungstenite::connect_async(request).await?;
    info!("Connected to '{url}'");

    Ok(ws_stream)
}
