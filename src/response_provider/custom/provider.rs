use super::config::CustomProviderConfig;
use crate::protocol_types;
use crate::response_provider::ResponseProvider;
use async_trait::async_trait;
use rhai::serde::to_dynamic;

pub struct CustomProvider {
    ast: rhai::AST,
    engine: rhai::Engine,
    client: reqwest::Client,
    url: String,
}

impl CustomProvider {
    pub fn new(config: &CustomProviderConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let engine = rhai::Engine::new();
        let ast = engine.compile_file(config.script.clone())?;
        let client = reqwest::Client::new();

        Ok(Self {
            ast,
            engine,
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
    ) -> Result<protocol_types::Message, Box<dyn std::error::Error>> {
        let messages_dynamic = to_dynamic(conversation_history)?;

        let mut scope = rhai::Scope::new();
        let request_body: rhai::Map = self
            .engine
            .call_fn(&mut scope, &self.ast, "build_request", (messages_dynamic,))
            .map_err(|e| format!("Rhai build_request error: {e}"))?;

        let json_body = rhai::serde::from_dynamic::<serde_json::Value>(&request_body.into())?;
        dbg!(&json_body);

        let response = self.client.post(&self.url).json(&json_body).send().await?;

        let response_body: serde_json::Value = response.json().await?;

        let body_dynamic = to_dynamic(response_body)?;

        let content: String = self
            .engine
            .call_fn(&mut scope, &self.ast, "parse_response", (body_dynamic,))
            .map_err(|e| format!("Rhai parse_response error: {e}"))?;

        Ok(protocol_types::Message {
            role: protocol_types::Role::Assistant,
            content,
        })
    }
}

