use std::time::{Instant, SystemTime};

use async_trait::async_trait;
use reqwest::header::HeaderMap;

use super::{
    ProviderApi, ProviderCompletion, ProviderError, ProviderResponse, ResponseProvider, retry,
};
use crate::protocol_types::Message;

/// Executes buffered HTTP completions with one shared retry policy.
pub struct HttpResponseProvider {
    client: reqwest::Client,
    api: Box<dyn ProviderApi>,
}

impl HttpResponseProvider {
    pub fn new(api: Box<dyn ProviderApi>, headers: &HeaderMap) -> Result<Self, ProviderError> {
        let client = reqwest::Client::builder()
            .default_headers(headers.clone())
            .build()?;
        Ok(Self { client, api })
    }

    pub(super) async fn execute_once(
        &self,
        messages: &[Message],
    ) -> Result<ProviderCompletion, ProviderError> {
        let request = self.api.build_request(messages).await?;
        let started = Instant::now();
        let response = self
            .client
            .request(request.method, request.url)
            .headers(request.headers)
            .body(request.body)
            .send()
            .await?;
        // Capture dates at receipt, before buffering or adapter processing.
        let retry_after = retry::parse_retry_after(response.headers(), SystemTime::now());
        let status = response.status();
        let headers = response.headers().clone();
        let url = response.url().to_string();
        let body = response.bytes().await?.to_vec();
        let response = ProviderResponse {
            status,
            headers,
            body,
            url,
            elapsed: started.elapsed(),
        };
        self.api
            .parse_response(response)
            .await
            .map_err(|error| error.with_retry_after(retry_after))
    }
}

#[async_trait]
impl ResponseProvider for HttpResponseProvider {
    async fn generate_response(
        &self,
        messages: &[Message],
    ) -> Result<ProviderCompletion, ProviderError> {
        retry::run(|| self.execute_once(messages)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol_types::Role;
    use crate::response_provider::{
        ProviderRequest,
        test_support::{StubApi, serve_responses},
    };
    use serde_json::json;
    use std::time::Duration;

    fn completion() -> ProviderCompletion {
        Message {
            role: Role::Assistant,
            content: "reply".into(),
        }
        .into()
    }

    #[tokio::test]
    async fn retries_rebuild_identical_requests_and_preserve_explicit_delay() {
        let (url, server) = serve_responses(vec![
            (429, Some("3600".into()), "not JSON".into()),
            (200, None, "{}".into()),
        ])
        .await;
        let api = StubApi::new(
            url,
            vec![
                Err(ProviderError::RateLimited {
                    message: "busy".into(),
                    retry_after: Some(Duration::ZERO),
                }),
                Ok(completion()),
            ],
        );
        let builds = api.builds.clone();
        let provider = HttpResponseProvider::new(Box::new(api), &HeaderMap::new()).unwrap();
        let messages = vec![Message {
            role: Role::User,
            content: "unchanged".into(),
        }];
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            provider.generate_response(&messages),
        )
        .await
        .expect("script delay must override long header delay")
        .unwrap();
        assert_eq!(response.message.content, "reply");
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].1,
            json!([{"role":"user", "content":"unchanged"}])
        );
        assert_eq!(requests[0].1, requests[1].1);
        let builds = builds.lock().unwrap();
        assert_eq!(builds.len(), 2);
        assert!(builds[1] - builds[0] >= Duration::from_secs(2));
    }

    #[tokio::test]
    async fn adapter_controls_http_error_interpretation_and_request_details() {
        struct InspectApi {
            url: String,
        }
        #[async_trait]
        impl ProviderApi for InspectApi {
            async fn build_request(
                &self,
                _messages: &[Message],
            ) -> Result<ProviderRequest, ProviderError> {
                let mut request = ProviderRequest::json(self.url.clone(), &json!({"value":42}))?;
                request.method = reqwest::Method::PUT;
                request
                    .headers
                    .insert("authorization", "Bearer adapter".parse().unwrap());
                Ok(request)
            }
            async fn parse_response(
                &self,
                response: ProviderResponse,
            ) -> Result<ProviderCompletion, ProviderError> {
                assert_eq!(response.status, reqwest::StatusCode::TOO_MANY_REQUESTS);
                assert_eq!(response.body, b"not JSON");
                assert_eq!(response.headers["retry-after"], "3600");
                assert_eq!(response.url, self.url);
                assert!(response.elapsed > Duration::ZERO);
                Ok(completion())
            }
        }
        let (url, server) =
            serve_responses(vec![(429, Some("3600".into()), "not JSON".into())]).await;
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer default".parse().unwrap());
        headers.insert("x-custom", "preserved".parse().unwrap());
        let provider = HttpResponseProvider::new(
            Box::new(InspectApi {
                url: format!("{url}/completion?key=value"),
            }),
            &headers,
        )
        .unwrap();
        assert_eq!(
            provider
                .generate_response(&[])
                .await
                .unwrap()
                .message
                .content,
            "reply"
        );
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 1);
        let (headers, body) = &requests[0];
        assert!(headers.starts_with("put /completion?key=value http/1.1"));
        assert!(headers.contains("authorization: bearer adapter"));
        assert!(!headers.contains("bearer default"));
        assert!(headers.contains("x-custom: preserved"));
        assert!(headers.contains("content-type: application/json"));
        assert_eq!(*body, json!({"value":42}));
    }

    #[tokio::test]
    async fn non_rate_limit_adapter_errors_return_without_retry() {
        let (url, server) = serve_responses(vec![(429, Some("3600".into()), "busy".into())]).await;
        let api = StubApi::new(
            url,
            vec![Err(ProviderError::Reported {
                reason: crate::protocol_types::common::CompletionErrorReason::AuthenticationFailed,
                message: "auth".into(),
            })],
        );
        let builds = api.builds.clone();
        let provider = HttpResponseProvider::new(Box::new(api), &HeaderMap::new()).unwrap();
        assert!(matches!(
            provider.generate_response(&[]).await,
            Err(ProviderError::Reported { .. })
        ));
        assert_eq!(builds.lock().unwrap().len(), 1);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn failed_body_read_returns_transport_error_without_parsing_or_retry() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 1];
            socket.read_exact(&mut buffer).await.unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort",
                )
                .await
                .unwrap();
        });
        let api = StubApi::new(url, vec![]);
        let builds = api.builds.clone();
        let provider = HttpResponseProvider::new(Box::new(api), &HeaderMap::new()).unwrap();
        assert!(matches!(
            provider.generate_response(&[]).await,
            Err(ProviderError::Network(_))
        ));
        assert_eq!(builds.lock().unwrap().len(), 1);
        server.await.unwrap();
    }

    #[test]
    fn standard_decoder_distinguishes_http_errors_from_invalid_json() {
        for (status, body) in [(429, "busy"), (500, "unavailable"), (200, "not JSON")] {
            let response = ProviderResponse {
                status: reqwest::StatusCode::from_u16(status).unwrap(),
                headers: HeaderMap::new(),
                body: body.as_bytes().to_vec(),
                url: "http://example.test".into(),
                elapsed: Duration::ZERO,
            };
            let error = response.decode_json::<serde_json::Value>().unwrap_err();
            match status {
                429 => assert!(matches!(
                    error,
                    ProviderError::RateLimited {
                        retry_after: None,
                        ..
                    }
                )),
                500 => assert!(matches!(error, ProviderError::Api(_))),
                _ => assert!(matches!(error, ProviderError::Parsing(_))),
            }
        }
    }
}
