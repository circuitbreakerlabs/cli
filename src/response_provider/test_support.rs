use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn serve_responses(
    responses: Vec<(u16, Option<String>, String)>,
) -> (
    String,
    tokio::task::JoinHandle<Vec<(String, serde_json::Value)>>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for (status, retry_after, body) in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = Vec::new();
            let mut chunk = [0; 4096];
            let header_end = loop {
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                buffer.extend_from_slice(&chunk[..count]);
                if let Some(index) = buffer.windows(4).position(|value| value == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let headers = String::from_utf8(buffer[..header_end].to_vec()).unwrap();
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            while buffer.len() < header_end + length {
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                buffer.extend_from_slice(&chunk[..count]);
            }
            let request = serde_json::from_slice(&buffer[header_end..header_end + length]).unwrap();
            let retry_header =
                retry_after.map_or_else(String::new, |value| format!("Retry-After: {value}\r\n"));
            socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\n{retry_header}Connection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            requests.push((headers.to_ascii_lowercase(), request));
        }
        requests
    });
    (url, server)
}

/// A mapping stub backed by real HTTP requests for executor and evaluation tests.
pub struct StubApi {
    url: String,
    outcomes: std::sync::Mutex<
        std::collections::VecDeque<Result<super::ProviderCompletion, super::ProviderError>>,
    >,
    pub builds: std::sync::Arc<std::sync::Mutex<Vec<tokio::time::Instant>>>,
    pub parsed: std::sync::Arc<tokio::sync::Notify>,
}

impl StubApi {
    pub fn new(
        url: String,
        outcomes: Vec<Result<super::ProviderCompletion, super::ProviderError>>,
    ) -> Self {
        Self {
            url,
            outcomes: std::sync::Mutex::new(outcomes.into()),
            builds: std::sync::Arc::default(),
            parsed: std::sync::Arc::default(),
        }
    }
}

#[async_trait::async_trait]
impl super::ProviderApi for StubApi {
    async fn build_request(
        &self,
        messages: &[crate::protocol_types::Message],
    ) -> Result<super::ProviderRequest, super::ProviderError> {
        self.builds
            .lock()
            .unwrap()
            .push(tokio::time::Instant::now());
        super::ProviderRequest::json(self.url.clone(), &messages)
    }

    async fn parse_response(
        &self,
        _response: super::ProviderResponse,
    ) -> Result<super::ProviderCompletion, super::ProviderError> {
        self.parsed.notify_one();
        self.outcomes
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected extra attempt")
    }
}
