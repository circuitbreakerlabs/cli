use crate::consts::headers;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use url::Url;

pub type WebSocketConnection = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub async fn connect(
    base_url: &str,
    evaluation_type: crate::evaluations::EvaluationType,
    api_key: &str,
) -> Result<WebSocketConnection, Box<dyn std::error::Error>> {
    let endpoint = crate::consts::endpoints::endpoint_from_evaluation_type(&evaluation_type);
    let mut url = Url::parse(format!("{base_url}/{endpoint}").as_str())?;

    url.set_path(
        url.path()
            .split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("/")
            .as_str(),
    );

    let mut request = url.as_str().into_client_request()?;
    request
        .headers_mut()
        .insert(headers::CBL_API_KEY, api_key.parse()?);

    let (ws_stream, _resp) = tokio_tungstenite::connect_async(request).await?;
    tracing::info!("Connected to '{url}'");

    Ok(ws_stream)
}
