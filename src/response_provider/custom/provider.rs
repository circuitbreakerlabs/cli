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
