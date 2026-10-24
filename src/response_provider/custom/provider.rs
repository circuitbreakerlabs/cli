use super::config::CustomProviderConfig;
use crate::protocol_types;
use crate::protocol_types::common::CompletionErrorReason;
use crate::response_provider::{ProviderCompletion, ProviderError, ResponseProvider};
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use rhai::serde::to_dynamic;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc, time::Instant};

#[derive(Serialize)]
struct HttpResponse {
    status: u16,
    headers: BTreeMap<String, Vec<String>>,
    body: serde_json::Value,
    body_text: String,
    body_parse_error: Option<String>,
    url: String,
    elapsed_ms: i64,
}

impl HttpResponse {
    async fn from_response(
        response: reqwest::Response,
        started: Instant,
    ) -> Result<Self, ProviderError> {
        let status = response.status().as_u16();
        let url = response.url().to_string();
        let headers: BTreeMap<String, Vec<String>> = response
            .headers()
            .keys()
            .map(|name| {
                let values = response
                    .headers()
                    .get_all(name)
                    .iter()
                    .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
                    .collect();
                (name.as_str().to_string(), values)
            })
            .collect();
        let bytes = response.bytes().await?;
        let elapsed_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
        let (body, body_parse_error) = match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(body) => (body, None),
            Err(error) => (serde_json::Value::Null, Some(error.to_string())),
        };
        Ok(Self {
            status,
            headers,
            body,
            body_text: String::from_utf8_lossy(&bytes).into_owned(),
            body_parse_error,
            url,
            elapsed_ms,
        })
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ParsedResponse {
    Success {
        content: String,
    },
    Error {
        reason: CompletionErrorReason,
        message: String,
    },
}

impl TryFrom<&rhai::Dynamic> for ProviderCompletion {
    type Error = ProviderError;

    fn try_from(value: &rhai::Dynamic) -> Result<Self, Self::Error> {
        let result: ParsedResponse = rhai::serde::from_dynamic(value).map_err(|e| {
            ProviderError::Parsing(format!("parse_response must return #{{type: \"success\", content: String}} or #{{type: \"error\", reason: valid error reason, message: String}}: {e}"))
        })?;
        match result {
            ParsedResponse::Success { content } if content.is_empty() => {
                Err(ProviderError::Parsing(
                    "parse_response returned an empty string for content".to_string(),
                ))
            }
            ParsedResponse::Success { content } => Ok(protocol_types::Message {
                role: protocol_types::Role::Assistant,
                content,
            }
            .into()),
            ParsedResponse::Error { reason, message } => {
                Err(ProviderError::Reported { reason, message })
            }
        }
    }
}

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
    ) -> Result<ProviderCompletion, ProviderError> {
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
        let started = Instant::now();
        let response = self.client.post(&self.url).json(&json_body).send().await?;
        let response = HttpResponse::from_response(response, started).await?;
        let response_dynamic =
            to_dynamic(response).map_err(|e| ProviderError::Parsing(e.to_string()))?;
        let engine = self.engine.clone();
        let ast = self.ast.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut scope = rhai::Scope::new();
            engine
                .call_fn::<rhai::Dynamic>(&mut scope, &ast, "parse_response", (response_dynamic,))
                .map_err(|e| ProviderError::Script(format!("parse_response: {e}")))
        })
        .await
        .map_err(|e| ProviderError::Script(format!("Spawn error: {e}")))??;

        ProviderCompletion::try_from(&result)
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
        spawn_http_server(
            200,
            vec![("Content-Type".into(), b"application/json".to_vec())],
            serde_json::to_vec(&response_body).unwrap(),
        )
        .await
    }

    async fn spawn_http_server(
        status: u16,
        headers: Vec<(String, Vec<u8>)>,
        response_body: Vec<u8>,
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

            let request_headers = std::str::from_utf8(&buffer[..header_end])
                .expect("request headers should be valid utf-8");
            let content_length = request_headers
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

            let mut response = format!(
                "HTTP/1.1 {status} Test\r\ncontent-length: {}\r\nconnection: close\r\n",
                response_body.len(),
            )
            .into_bytes();
            for (name, value) in headers {
                response.extend_from_slice(name.as_bytes());
                response.extend_from_slice(b": ");
                response.extend_from_slice(&value);
                response.extend_from_slice(b"\r\n");
            }
            response.extend_from_slice(b"\r\n");
            response.extend_from_slice(&response_body);
            socket
                .write_all(&response)
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

        let Err(err) = CustomProvider::new(&config, &HeaderMap::new()) else {
            panic!("invalid script should fail to compile")
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

                fn parse_response(response) {
                    #{type: "success", content: response.body["reply"]}
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

        assert!(matches!(response.message.role, Role::Assistant));
        assert_eq!(response.message.content, "translated response");
        assert_eq!(response.model_id, None);
        assert_eq!(response.tokens_used, None);
        assert_eq!(response.finish_reason, None);
        assert_eq!(response.provider_response_id, None);
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

                fn parse_response(response) {
                    #{type: "success", content: response.body["reply"]}
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

                fn parse_response(response) {
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

    #[tokio::test]
    async fn generate_response_rejects_empty_parse_response_output() {
        let path = write_temp_script(
            r#"
                fn build_request(messages) {
                    #{ "ok": true }
                }

                fn parse_response(response) {
                    #{type: "success", content: ""}
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
            .expect_err("empty parse_response output should be rejected");

        assert!(matches!(err, ProviderError::Parsing(_)));
        assert!(err.to_string().contains("empty string"));

        let _ = std::fs::remove_file(path);
    }
    async fn run_script(
        script: &str,
        url: String,
    ) -> Result<crate::response_provider::ProviderCompletion, ProviderError> {
        let path = write_temp_script(script);
        let provider = CustomProvider::new(
            &CustomProviderConfig {
                url,
                script: path.clone(),
            },
            &HeaderMap::new(),
        )
        .unwrap();
        let result = provider.generate_response(&[]).await;
        std::fs::remove_file(path).unwrap();
        result
    }

    #[tokio::test]
    async fn response_context_preserves_headers_status_text_and_redirect_url() {
        let (destination, final_server) = spawn_http_server(
            429,
            vec![
                ("Retry-After".into(), b"12".to_vec()),
                ("X-Repeated".into(), b"first".to_vec()),
                ("x-repeated".into(), b"second".to_vec()),
                ("X-Bytes".into(), vec![0xff]),
            ],
            b"<html>busy</html>\xff".to_vec(),
        )
        .await;
        let (url, redirect_server) = spawn_http_server(
            307,
            vec![("Location".into(), destination.as_bytes().to_vec())],
            vec![],
        )
        .await;
        let script = r#"
            fn build_request(messages) { #{} }
            fn parse_response(response) {
                if response.status != 429 || response.headers["retry-after"] != ["12"]
                    || response.headers["x-repeated"] != ["first", "second"]
                    || response.headers.contains("missing") || response.headers.contains("Retry-After")
                    || response.headers["x-bytes"] != ["�"]
                    || response.body != () || type_of(response.body_parse_error) != "string"
                    || response.body_text != "<html>busy</html>�" || response.elapsed_ms < 0 {
                    throw "incorrect response context";
                }
                #{type: "success", content: response.url}
            }
        "#;
        let completion = run_script(script, url).await.unwrap();
        assert_eq!(completion.message.content, format!("{destination}/"));
        redirect_server.await.unwrap();
        final_server.await.unwrap();
    }

    #[tokio::test]
    async fn response_context_distinguishes_null_empty_and_malformed_bodies() {
        for (body, valid_json) in [("null", true), ("", false), ("{bad", false)] {
            let (url, server) = spawn_http_server(200, vec![], body.as_bytes().to_vec()).await;
            let script = format!(
                r#"
                fn build_request(messages) {{ #{{}} }}
                fn parse_response(response) {{
                    if response.body != () || (response.body_parse_error == ()) != {valid_json} {{
                        throw "incorrect JSON parse state";
                    }}
                    #{{type: "success", content: "checked:" + response.body_text}}
                }}
            "#
            );
            let completion = run_script(&script, url).await.unwrap();
            assert_eq!(completion.message.content, format!("checked:{body}"));
            server.await.unwrap();
        }
    }

    #[test]
    fn rejects_malformed_results_with_contract_diagnostics() {
        for expression in [
            r#""legacy string""#,
            "()",
            "42",
            "#{}",
            r#"#{type: "other", content: "reply"}"#,
            r#"#{type: "success"}"#,
            r#"#{type: "success", content: 42}"#,
            r#"#{type: "success", content: "reply", extra: true}"#,
            r#"#{type: "error", reason: "other", message: "failure"}"#,
            r#"#{type: "error", reason: "unknown"}"#,
            r#"#{type: "error", reason: "unknown", message: 42}"#,
            r#"#{type: "error", reason: "unknown", message: "failure", content: "reply"}"#,
        ] {
            let value = rhai::Engine::new()
                .eval::<rhai::Dynamic>(expression)
                .unwrap();
            let error = super::ProviderCompletion::try_from(&value).unwrap_err();
            assert!(
                matches!(error, ProviderError::Parsing(_)),
                "{expression}: {error}"
            );
            assert!(error.to_string().contains("parse_response must return"));
        }
    }

    #[tokio::test]
    async fn reported_reasons_survive_http_and_rust_mapping() {
        use crate::protocol_types::common::CompletionErrorReason;
        for reason in [
            "model_timeout",
            "model_unreachable",
            "invalid_response",
            "rate_limited",
            "authentication_failed",
            "unknown",
        ] {
            let (url, server) = spawn_json_server(json!({"reason": reason})).await;
            let error = run_script(
                r#"
                fn build_request(messages) { #{} }
                fn parse_response(response) {
                    #{type: "error", reason: response.body.reason, message: "Rate auth key"}
                }
            "#,
                url,
            )
            .await
            .unwrap_err();
            assert!(
                matches!(&error, ProviderError::Reported { message, .. } if message == "Rate auth key")
            );
            assert_eq!(
                serde_json::to_value(CompletionErrorReason::from(&error)).unwrap(),
                json!(reason)
            );
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn rejects_positional_signature() {
        let (url, server) = spawn_json_server(json!({})).await;
        let error = run_script(
            r#"
            fn build_request(messages) { #{} }
            fn parse_response(body, status) { #{type: "success", content: "old"} }
        "#,
            url,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, ProviderError::Script(_)));
        assert!(error.to_string().contains("parse_response"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn bundled_scripts_handle_success_and_error_responses() {
        let fixtures = [
            (
                include_str!("../../../examples/providers/ollama_chat.rhai"),
                json!({"message":{"content":"hello"}}),
            ),
            (
                include_str!("../../../examples/providers/ollama_completions.rhai"),
                json!({"choices":[{"message":{"content":"hello"}}]}),
            ),
            (
                include_str!("../../../examples/providers/openai_completions.rhai"),
                json!({"choices":[{"message":{"content":"hello"}}]}),
            ),
            (
                include_str!("../../../examples/providers/openai_responses.rhai"),
                json!({"output":[{"content":[{"text":"hello"}]}]}),
            ),
        ];
        for (script, body) in fixtures {
            let (url, server) = spawn_json_server(body).await;
            assert_eq!(
                run_script(script, url).await.unwrap().message.content,
                "hello"
            );
            server.await.unwrap();
            for (status, body, expected) in [
                (401, "", "authentication_failed"),
                (429, "busy", "rate_limited"),
                (408, "", "model_timeout"),
                (504, "<html>gateway</html>", "model_timeout"),
                (500, "{}", "unknown"),
                (200, r#"{"error":{"message":"oops"}}"#, "unknown"),
                (200, r#"{"error":"oops"}"#, "unknown"),
                (200, "invalid", "invalid_response"),
                (200, "{}", "invalid_response"),
                (200, "null", "invalid_response"),
            ] {
                let (url, server) =
                    spawn_http_server(status, vec![], body.as_bytes().to_vec()).await;
                let error = run_script(script, url).await.unwrap_err();
                assert!(matches!(&error, ProviderError::Reported { .. }), "{error}");
                assert_eq!(
                    serde_json::to_value(
                        crate::protocol_types::common::CompletionErrorReason::from(&error)
                    )
                    .unwrap(),
                    json!(expected)
                );
                server.await.unwrap();
            }
        }
    }
}
