use async_openai::types::chat::{
    CreateChatCompletionRequest, ReasoningEffort, ServiceTier, StopConfiguration,
};
use std::collections::HashMap;

const OPENAI_API_KEY: &str = "OPENAI_API_KEY";

fn masked_api_key() -> String {
    let var = std::env::var(OPENAI_API_KEY).ok();

    let masked = match var {
        Some(v) if v.len() > 4 => format!("****{}", &v[v.len() - 4..]),
        Some(_) => "****".to_string(),
        None => String::new(),
    };

    format!("[env: {OPENAI_API_KEY}={masked}]")
}

#[derive(Clone, Debug, clap::Args)]
#[clap(next_help_heading = "Required Options")]
pub struct RequiredOpenAIArgs {
    #[allow(clippy::doc_markdown)]
    /// OpenAI API key
    #[arg(long, env = OPENAI_API_KEY, help = masked_api_key(), hide_env = true)]
    pub api_key: String,

    #[allow(clippy::doc_markdown)]
    /// OpenAI model name (e.g., gpt-4o, gpt-4-turbo, gpt-3.5-turbo)
    #[arg(long)]
    pub model: String,
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum OpenAIReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

impl From<OpenAIReasoningEffort> for ReasoningEffort {
    fn from(effort: OpenAIReasoningEffort) -> Self {
        match effort {
            OpenAIReasoningEffort::None => ReasoningEffort::None,
            OpenAIReasoningEffort::Minimal => ReasoningEffort::Minimal,
            OpenAIReasoningEffort::Low => ReasoningEffort::Low,
            OpenAIReasoningEffort::Medium => ReasoningEffort::Medium,
            OpenAIReasoningEffort::High => ReasoningEffort::High,
            OpenAIReasoningEffort::Xhigh => ReasoningEffort::Xhigh,
        }
    }
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum OpenAIServiceTier {
    Auto,
    Default,
    Flex,
    Scale,
    Priority,
}

impl From<OpenAIServiceTier> for ServiceTier {
    fn from(tier: OpenAIServiceTier) -> Self {
        match tier {
            OpenAIServiceTier::Auto => ServiceTier::Auto,
            OpenAIServiceTier::Default => ServiceTier::Default,
            OpenAIServiceTier::Flex => ServiceTier::Flex,
            OpenAIServiceTier::Scale => ServiceTier::Scale,
            OpenAIServiceTier::Priority => ServiceTier::Priority,
        }
    }
}

#[derive(Clone, Debug, clap::Args)]
#[clap(next_help_heading = "Extra Options")]
pub struct OptionalOpenAIArgs {
    #[allow(clippy::doc_markdown)]
    /// OpenAI API base URL for compatible endpoints
    #[arg(
        long,
        env = "OPENAI_BASE_URL",
        default_value = "https://api.openai.com/v1"
    )]
    pub base_url: String,

    #[allow(clippy::doc_markdown)]
    /// OpenAI organization ID
    #[arg(long, env = "OPENAI_ORG_ID")]
    pub org_id: Option<String>,

    /// What sampling temperature to use, between 0 and 2
    #[arg(long)]
    pub temperature: Option<f32>,

    /// An alternative to sampling with temperature, called nucleus sampling
    #[arg(long)]
    pub top_p: Option<f32>,

    /// An upper bound for the number of tokens that can be generated for a completion
    #[arg(long)]
    pub max_completion_tokens: Option<u32>,

    /// How many chat completion choices to generate for each input message
    #[arg(long)]
    pub n: Option<u8>,

    /// Number between -2.0 and 2.0 to penalize new tokens based on existing frequency
    #[arg(long)]
    pub frequency_penalty: Option<f32>,

    /// Number between -2.0 and 2.0 to penalize new tokens based on whether they appear
    #[arg(long)]
    pub presence_penalty: Option<f32>,

    /// Whether to return log probabilities of the output tokens
    #[arg(long)]
    pub logprobs: Option<bool>,

    /// An integer between 0 and 20 specifying the number of most likely tokens to return
    #[arg(long)]
    pub top_logprobs: Option<u8>,

    /// Up to 4 sequences where the API will stop generating further tokens
    #[arg(long, value_delimiter = ',')]
    pub stop: Option<Vec<String>>,

    /// Modify the likelihood of specified tokens appearing in the completion
    #[arg(long, value_parser = parse_logit_bias)]
    pub logit_bias: Option<HashMap<String, i8>>,

    /// Whether to store the output of this chat completion request
    #[arg(long)]
    pub store: Option<bool>,

    /// Specifies the processing type used for serving the request
    #[arg(long)]
    pub service_tier: Option<OpenAIServiceTier>,

    /// Constrains effort on reasoning for reasoning models (minimal, low, medium, high)
    #[arg(long)]
    pub reasoning_effort: Option<OpenAIReasoningEffort>,
}

fn parse_logit_bias(s: &str) -> Result<HashMap<String, i8>, String> {
    let mut map = HashMap::new();
    for pair in s.split(',') {
        let parts: Vec<&str> = pair.split(':').collect();
        if parts.len() != 2 {
            return Err("Invalid logit_bias format. Expected token_id:bias_value".to_string());
        }
        let token_id = parts[0].to_string();

        let bias = match parts[1].parse() {
            Ok(b) if (-100..=100).contains(&b) => b,
            _ => return Err("Bias value must be an integer between -100 and 100".into()),
        };

        map.insert(token_id, bias);
    }
    Ok(map)
}

#[derive(Clone, Debug, clap::Args)]
pub struct OpenAIProviderConfig {
    #[clap(flatten)]
    pub required: RequiredOpenAIArgs,

    #[clap(flatten)]
    pub optional: OptionalOpenAIArgs,
}

impl OpenAIProviderConfig {
    pub fn build_openai_config(&self) -> async_openai::config::OpenAIConfig {
        let mut config =
            async_openai::config::OpenAIConfig::new().with_api_key(&self.required.api_key);

        if let Some(ref org_id) = self.optional.org_id {
            config = config.with_org_id(org_id);
        }

        if self.optional.base_url != "https://api.openai.com/v1" {
            config = config.with_api_base(&self.optional.base_url);
        }

        config
    }

    pub fn build_request(
        &self,
        messages: Vec<async_openai::types::chat::ChatCompletionRequestMessage>,
    ) -> CreateChatCompletionRequest {
        let mut request = CreateChatCompletionRequest {
            model: self.required.model.clone(),
            messages,
            ..Default::default()
        };

        if let Some(temperature) = self.optional.temperature {
            request.temperature = Some(temperature);
        }

        if let Some(top_p) = self.optional.top_p {
            request.top_p = Some(top_p);
        }

        if let Some(max_completion_tokens) = self.optional.max_completion_tokens {
            request.max_completion_tokens = Some(max_completion_tokens);
        }

        if let Some(n) = self.optional.n {
            request.n = Some(n);
        }

        if let Some(frequency_penalty) = self.optional.frequency_penalty {
            request.frequency_penalty = Some(frequency_penalty);
        }

        if let Some(presence_penalty) = self.optional.presence_penalty {
            request.presence_penalty = Some(presence_penalty);
        }

        if let Some(logprobs) = self.optional.logprobs {
            request.logprobs = Some(logprobs);
        }

        if let Some(top_logprobs) = self.optional.top_logprobs {
            request.top_logprobs = Some(top_logprobs);
        }

        if let Some(ref stop) = self.optional.stop
            && !stop.is_empty()
        {
            if stop.len() == 1 {
                request.stop = Some(StopConfiguration::String(stop[0].clone()));
            } else {
                request.stop = Some(StopConfiguration::StringArray(stop.clone()));
            }
        }

        if let Some(ref logit_bias) = self.optional.logit_bias {
            request.logit_bias = Some(logit_bias.clone());
        }

        if let Some(store) = self.optional.store {
            request.store = Some(store);
        }

        if let Some(ref service_tier) = self.optional.service_tier {
            request.service_tier = Some(service_tier.clone().into());
        }

        if let Some(ref reasoning_effort) = self.optional.reasoning_effort {
            request.reasoning_effort = Some(reasoning_effort.clone().into());
        }

        request
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OpenAIProviderConfig, OpenAIReasoningEffort, OpenAIServiceTier, OptionalOpenAIArgs,
        RequiredOpenAIArgs, parse_logit_bias,
    };
    use async_openai::config::Config;
    use async_openai::types::chat::StopConfiguration;
    use reqwest::header::AUTHORIZATION;
    use std::collections::HashMap;

    fn make_config() -> OpenAIProviderConfig {
        OpenAIProviderConfig {
            required: RequiredOpenAIArgs {
                api_key: "test-key".to_string(),
                model: "gpt-test".to_string(),
            },
            optional: OptionalOpenAIArgs {
                base_url: "https://example.com/v1".to_string(),
                org_id: Some("org_123".to_string()),
                temperature: Some(0.7),
                top_p: Some(0.8),
                max_completion_tokens: Some(256),
                n: Some(2),
                frequency_penalty: Some(0.3),
                presence_penalty: Some(0.4),
                logprobs: Some(true),
                top_logprobs: Some(5),
                stop: Some(vec!["END".to_string()]),
                logit_bias: Some(HashMap::from([("42".to_string(), 7)])),
                store: Some(true),
                service_tier: Some(OpenAIServiceTier::Priority),
                reasoning_effort: Some(OpenAIReasoningEffort::High),
            },
        }
    }

    #[test]
    fn parses_logit_bias_pairs() {
        let bias = parse_logit_bias("12:5,42:-7").expect("logit bias should parse");

        assert_eq!(bias["12"], 5);
        assert_eq!(bias["42"], -7);
    }

    #[test]
    fn rejects_out_of_range_logit_bias() {
        let err = parse_logit_bias("12:101").expect_err("logit bias should fail");

        assert!(err.contains("between -100 and 100"));
    }

    #[test]
    fn build_openai_config_applies_base_url_org_and_auth() {
        let config = make_config().build_openai_config();
        let headers = config.headers();

        assert_eq!(
            config.url("/chat/completions"),
            "https://example.com/v1/chat/completions"
        );
        assert_eq!(config.org_id(), "org_123");
        assert_eq!(headers[AUTHORIZATION], "Bearer test-key");
    }

    #[test]
    fn build_request_maps_optional_fields() {
        let request = make_config().build_request(Vec::new());

        assert_eq!(request.model, "gpt-test");
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(request.top_p, Some(0.8));
        assert_eq!(request.max_completion_tokens, Some(256));
        assert_eq!(request.n, Some(2));
        assert_eq!(request.frequency_penalty, Some(0.3));
        assert_eq!(request.presence_penalty, Some(0.4));
        assert_eq!(request.logprobs, Some(true));
        assert_eq!(request.top_logprobs, Some(5));
        assert_eq!(
            request.logit_bias.as_ref().and_then(|m| m.get("42")),
            Some(&7)
        );
        assert_eq!(request.store, Some(true));
        assert!(request.service_tier.is_some());
        assert!(request.reasoning_effort.is_some());
        assert!(matches!(
            request.stop,
            Some(StopConfiguration::String(ref value)) if value == "END"
        ));
    }

    #[test]
    fn build_request_uses_string_array_for_multiple_stop_sequences() {
        let mut config = make_config();
        config.optional.stop = Some(vec!["END".to_string(), "HALT".to_string()]);

        let request = config.build_request(Vec::new());

        assert!(matches!(
            request.stop,
            Some(StopConfiguration::StringArray(ref value))
                if value == &vec!["END".to_string(), "HALT".to_string()]
        ));
    }
}
