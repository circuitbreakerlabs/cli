use super::err::WebSocketError;
use crate::consts::headers;
use reqwest::header::InvalidHeaderValue;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use url::Url;

pub type WebSocketConnection = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub async fn connect(
    base_url: &str,
    evaluation_type: crate::evaluations::EvaluationType,
    api_key: &str,
) -> Result<WebSocketConnection, WebSocketError> {
    let endpoint = crate::consts::endpoints::endpoint_from_evaluation_type(&evaluation_type);
    let mut url = Url::parse(format!("{base_url}/{endpoint}").as_str())
        .map_err(|e| WebSocketError::Parsing(e.to_string()))?;

    url.set_path(
        url.path()
            .split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("/")
            .as_str(),
    );

    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|e| WebSocketError::Parsing(e.to_string()))?;
    request.headers_mut().insert(
        headers::CBL_API_KEY,
        api_key
            .parse()
            .map_err(|e: InvalidHeaderValue| WebSocketError::Parsing(e.to_string()))?,
    );

    let (ws_stream, _resp) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| WebSocketError::Connect(e.to_string()))?;
    tracing::info!("Connected to '{url}'");

    Ok(ws_stream)
}
