use super::config::CustomProviderConfig;
use crate::protocol_types;
use crate::response_provider::{ProviderError, ResponseProvider};
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use rhai::serde::to_dynamic;
use std::sync::Arc;

pub struct CustomProvider {
    ast: Arc<rhai::AST>,
    engine: Arc<rhai::Engine>,
    client: reqwest::Client,
    url: String,
}

impl CustomProvider {
    pub fn new(config: &CustomProviderConfig, headers: &HeaderMap) -> Result<Self, ProviderError> {
        let mut engine = rhai::Engine::new();
        let ast = Arc::new(
            engine
                .compile_file(config.script.clone())
                .map_err(|e| ProviderError::Script(e.to_string()))?,
        );
        let client = reqwest::Client::builder()
            .default_headers(headers.clone())
            .build()?;

        engine.on_print(|s| tracing::info!("[rhai] {s}"));
        engine.on_debug(|s, src, pos| {
            tracing::debug!("[rhai] {s} @ {src:?}:{pos}");
        });

        Ok(Self {
            engine: Arc::new(engine),
            ast,
            client,
            url: config.url.clone(),
        })
    }
}

#[async_trait]
impl ResponseProvider for CustomProvider {
    async fn generate_response(
        &self,
        conversation_history: &[protocol_types::Message],
    ) -> Result<protocol_types::Message, ProviderError> {
        let messages_dynamic =
            to_dynamic(conversation_history).map_err(|e| ProviderError::Parsing(e.to_string()))?;
        let engine = self.engine.clone();
        let ast = self.ast.clone();
        let request_body = tokio::task::spawn_blocking(move || {
            let mut scope = rhai::Scope::new();
            engine
                .call_fn::<rhai::Map>(&mut scope, &ast, "build_request", (messages_dynamic,))
                .map_err(|e| ProviderError::Script(format!("build_request: {e}")))
        })
        .await
        .map_err(|e| ProviderError::Script(format!("Spawn error: {e}")))??;

        let json_body = rhai::serde::from_dynamic::<serde_json::Value>(&request_body.into())
            .map_err(|e| ProviderError::Parsing(e.to_string()))?;
        let response_body: serde_json::Value = self
            .client
            .post(&self.url)
            .json(&json_body)
            .send()
            .await?
            .json()
            .await?;

        let response_body_dynamic =
            to_dynamic(response_body).map_err(|e| ProviderError::Parsing(e.to_string()))?;
        let engine = self.engine.clone();
        let ast = self.ast.clone();
        let content = tokio::task::spawn_blocking(move || {
            let mut scope = rhai::Scope::new();
            engine
                .call_fn::<String>(&mut scope, &ast, "parse_response", (response_body_dynamic,))
                .map_err(|e| ProviderError::Script(format!("parse_response: {e}")))
        })
        .await
        .map_err(|e| ProviderError::Script(format!("Spawn error: {e}")))??;

        Ok(protocol_types::Message {
            role: protocol_types::Role::Assistant,
            content,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::CustomProvider;
    use crate::protocol_types::{Message, Role};
    use crate::response_provider::{CustomProviderConfig, ProviderError, ResponseProvider};
    use reqwest::header::HeaderMap;
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    fn write_temp_script(contents: &str) -> PathBuf {
        let unique = format!(
            "{}_{}_{}.rhai",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after epoch")
                .as_nanos(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed),
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::write(&path, contents).expect("test script should be written");
        path
    }

    async fn spawn_json_server(
        response_body: serde_json::Value,
    ) -> (String, tokio::task::JoinHandle<serde_json::Value>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("server should bind");
        let addr = listener
            .local_addr()
            .expect("server should have local addr");

        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("server should accept");
            let mut buffer = Vec::new();
            let mut temp = [0_u8; 1024];
            let header_end;

            loop {
                let n = socket
                    .read(&mut temp)
                    .await
                    .expect("server should read request");
                assert!(n > 0, "client closed before sending full request");
                buffer.extend_from_slice(&temp[..n]);
                if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                    header_end = pos + 4;
                    break;
                }
            }

            let headers = std::str::from_utf8(&buffer[..header_end])
                .expect("request headers should be valid utf-8");
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    if name.eq_ignore_ascii_case("content-length") {
                        Some(
                            value
                                .trim()
                                .parse::<usize>()
                                .expect("content-length should parse"),
                        )
                    } else {
                        None
                    }
                })
                .expect("request should have content-length");

            while buffer.len() < header_end + content_length {
                let n = socket
                    .read(&mut temp)
                    .await
                    .expect("server should continue reading body");
                assert!(n > 0, "client closed before body was complete");
                buffer.extend_from_slice(&temp[..n]);
            }

            let body = &buffer[header_end..header_end + content_length];
            let request_json: serde_json::Value =
                serde_json::from_slice(body).expect("request body should be valid json");

            let response_json =
                serde_json::to_string(&response_body).expect("response json should serialize");
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_json.len(),
                response_json
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("server should write response");

            request_json
        });

        (format!("http://{addr}"), handle)
    }

    #[test]
    fn new_rejects_invalid_rhai_script() {
        let path = write_temp_script("fn build_request(messages) {");
        let config = CustomProviderConfig {
            url: "http://127.0.0.1".to_string(),
            script: path.clone(),
        };

        let err = match CustomProvider::new(&config, &HeaderMap::new()) {
            Ok(_) => panic!("invalid script should fail to compile"),
            Err(err) => err,
        };

        assert!(matches!(err, ProviderError::Script(_)));

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn generate_response_uses_rhai_request_and_response_translation() {
        let path = write_temp_script(
            r#"
                fn build_request(messages) {
                    #{
                        "message_count": messages.len,
                        "last_role": messages[messages.len - 1]["role"].to_string(),
                        "last_content": messages[messages.len - 1]["content"].to_string()
                    }
                }

                fn parse_response(body) {
                    body["reply"].to_string()
                }
            "#,
        );
        let (url, server_handle) =
            spawn_json_server(json!({ "reply": "translated response" })).await;
        let config = CustomProviderConfig {
            url,
            script: path.clone(),
        };
        let provider =
            CustomProvider::new(&config, &HeaderMap::new()).expect("provider should build");
        let messages = vec![
            Message {
                role: Role::System,
                content: "be safe".to_string(),
            },
            Message {
                role: Role::User,
                content: "hello".to_string(),
            },
        ];

        let response = provider
            .generate_response(&messages)
            .await
            .expect("response should be generated");
        let request_json = server_handle.await.expect("server task should complete");

        assert!(matches!(response.role, Role::Assistant));
        assert_eq!(response.content, "translated response");
        assert_eq!(request_json["message_count"], json!(2));
        assert_eq!(request_json["last_role"], json!("user"));
        assert_eq!(request_json["last_content"], json!("hello"));

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn generate_response_reports_build_request_script_errors() {
        let path = write_temp_script(
            r#"
                fn build_request(messages) {
                    throw("bad request")
                }

                fn parse_response(body) {
                    body["reply"].to_string()
                }
            "#,
        );
        let config = CustomProviderConfig {
            url: "http://127.0.0.1:9".to_string(),
            script: path.clone(),
        };
        let provider =
            CustomProvider::new(&config, &HeaderMap::new()).expect("provider should build");

        let err = provider
            .generate_response(&[Message {
                role: Role::User,
                content: "hello".to_string(),
            }])
            .await
            .expect_err("script error should be surfaced");

        assert!(matches!(err, ProviderError::Script(_)));
        assert!(err.to_string().contains("build_request"));

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn generate_response_reports_parse_response_script_errors() {
        let path = write_temp_script(
            r#"
                fn build_request(messages) {
                    #{ "ok": true }
                }

                fn parse_response(body) {
                    throw("bad response")
                }
            "#,
        );
        let (url, _server_handle) = spawn_json_server(json!({ "reply": "ignored" })).await;
        let config = CustomProviderConfig {
            url,
            script: path.clone(),
        };
        let provider =
            CustomProvider::new(&config, &HeaderMap::new()).expect("provider should build");

        let err = provider
            .generate_response(&[Message {
                role: Role::User,
                content: "hello".to_string(),
            }])
            .await
            .expect_err("script error should be surfaced");

        assert!(matches!(err, ProviderError::Script(_)));
        assert!(err.to_string().contains("parse_response"));

        let _ = std::fs::remove_file(path);
    }
}
