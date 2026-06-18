use clap;
use ollama_rs::models::ModelOptions;

#[derive(Clone, Debug, clap::Args)]
#[clap(next_help_heading = "Required Options")]
pub struct RequiredOllamaArgs {
    /// Ollama model name. Can be repeated for parallel evaluation.
    #[arg(long, required = true)]
    pub model: Vec<String>,
}

#[derive(Clone, Debug, clap::Args)]
#[clap(next_help_heading = "Extra Options")]
pub struct OptionalOllamaArgs {
    /// Ollama Base URL
    #[arg(
        long,
        env = "OLLAMA_BASE_URL",
        default_value = "http://localhost:11434"
    )]
    pub base_url: String,

    #[arg(help_heading = "Extra Options")]

    /// Return log probabilities for each token
    #[arg(long)]
    pub logprobs: Option<bool>,

    /// Enable Mirostat sampling (0 = disabled, 1 = Mirostat, 2 = Mirostat 2.0)
    #[arg(long)]
    pub mirostat: Option<u8>,

    /// Mirostat learning rate (default: 0.1)
    #[arg(long)]
    pub mirostat_eta: Option<f32>,

    /// Mirostat tau - controls balance between coherence and diversity (default: 5.0)
    #[arg(long)]
    pub mirostat_tau: Option<f32>,

    /// Size of the context window (default: 2048)
    #[arg(long)]
    pub num_ctx: Option<u64>,

    /// Number of layers to send to GPU(s)
    #[arg(long)]
    pub num_gpu: Option<u32>,

    /// Number of GQA groups in transformer layer
    #[arg(long)]
    pub num_gqa: Option<u32>,

    /// Maximum number of tokens to predict (default: 128, -1 = infinite, -2 = fill context)
    #[arg(long)]
    pub num_predict: Option<i32>,

    /// Number of threads to use during computation
    #[arg(long)]
    pub num_thread: Option<u32>,

    /// How far back to look to prevent repetition (default: 64, 0 = disabled, -1 = `num_ctx`)
    #[arg(long)]
    pub repeat_last_n: Option<i32>,

    /// How strongly to penalize repetitions (default: 1.1)
    #[arg(long)]
    pub repeat_penalty: Option<f32>,

    /// Random number seed for generation (default: 0)
    #[arg(long)]
    pub seed: Option<i32>,

    /// Stop sequences (can be specified multiple times)
    #[arg(long)]
    pub stop: Option<Vec<String>>,

    /// Model temperature - higher values make answers more creative (default: 0.8)
    #[arg(long)]
    pub temperature: Option<f32>,

    /// Tail free sampling - reduces impact of less probable tokens (default: 1)
    #[arg(long)]
    pub tfs_z: Option<f32>,

    /// Reduces probability of generating nonsense - higher values give more diverse answers (default: 40)
    #[arg(long)]
    pub top_k: Option<u32>,

    /// Works with top-k - higher values lead to more diverse text (default: 0.9)
    #[arg(long)]
    pub top_p: Option<f32>,
}

#[derive(Clone, Debug, clap::Args)]
pub struct OllamaProviderConfig {
    #[clap(flatten)]
    pub required: RequiredOllamaArgs,

    #[clap(flatten)]
    pub optional: OptionalOllamaArgs,
}

impl OllamaProviderConfig {
    pub fn model_ids(&self) -> &[String] {
        &self.required.model
    }

    pub fn with_model(&self, model: String) -> Self {
        let mut config = self.clone();
        config.required.model = vec![model];
        config
    }

    pub fn model(&self) -> &str {
        self.required
            .model
            .first()
            .expect("Ollama provider config should include a model")
    }

    pub fn build_model_options(&self) -> Option<ModelOptions> {
        let mut options = ModelOptions::default();
        let mut has_options = false;

        if let Some(mirostat) = self.optional.mirostat {
            options = options.mirostat(mirostat);
            has_options = true;
        }

        if let Some(mirostat_eta) = self.optional.mirostat_eta {
            options = options.mirostat_eta(mirostat_eta);
            has_options = true;
        }

        if let Some(mirostat_tau) = self.optional.mirostat_tau {
            options = options.mirostat_tau(mirostat_tau);
            has_options = true;
        }

        if let Some(num_ctx) = self.optional.num_ctx {
            options = options.num_ctx(num_ctx);
            has_options = true;
        }

        if let Some(num_gpu) = self.optional.num_gpu {
            options = options.num_gpu(num_gpu);
            has_options = true;
        }

        if let Some(num_gqa) = self.optional.num_gqa {
            options = options.num_gqa(num_gqa);
            has_options = true;
        }

        if let Some(num_predict) = self.optional.num_predict {
            options = options.num_predict(num_predict);
            has_options = true;
        }

        if let Some(num_thread) = self.optional.num_thread {
            options = options.num_thread(num_thread);
            has_options = true;
        }

        if let Some(repeat_last_n) = self.optional.repeat_last_n {
            options = options.repeat_last_n(repeat_last_n);
            has_options = true;
        }

        if let Some(repeat_penalty) = self.optional.repeat_penalty {
            options = options.repeat_penalty(repeat_penalty);
            has_options = true;
        }

        if let Some(seed) = self.optional.seed {
            options = options.seed(seed);
            has_options = true;
        }

        if let Some(stop) = &self.optional.stop {
            options = options.stop(stop.clone());
            has_options = true;
        }

        if let Some(temperature) = self.optional.temperature {
            options = options.temperature(temperature);
            has_options = true;
        }

        if let Some(tfs_z) = self.optional.tfs_z {
            options = options.tfs_z(tfs_z);
            has_options = true;
        }

        if let Some(top_k) = self.optional.top_k {
            options = options.top_k(top_k);
            has_options = true;
        }

        if let Some(top_p) = self.optional.top_p {
            options = options.top_p(top_p);
            has_options = true;
        }

        if has_options { Some(options) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::{OllamaProviderConfig, OptionalOllamaArgs, RequiredOllamaArgs};
    use serde_json::json;

    fn make_config() -> OllamaProviderConfig {
        OllamaProviderConfig {
            required: RequiredOllamaArgs {
                model: vec!["llama-test".to_string()],
            },
            optional: OptionalOllamaArgs {
                base_url: "http://localhost:11434".to_string(),
                logprobs: Some(true),
                mirostat: Some(2),
                mirostat_eta: Some(0.1),
                mirostat_tau: Some(5.0),
                num_ctx: Some(4096),
                num_gpu: Some(1),
                num_gqa: Some(8),
                num_predict: Some(128),
                num_thread: Some(4),
                repeat_last_n: Some(64),
                repeat_penalty: Some(1.2),
                seed: Some(7),
                stop: Some(vec!["END".to_string()]),
                temperature: Some(0.8),
                tfs_z: Some(1.1),
                top_k: Some(40),
                top_p: Some(0.9),
            },
        }
    }

    #[test]
    fn build_model_options_returns_none_when_no_options_are_set() {
        let config = OllamaProviderConfig {
            required: RequiredOllamaArgs {
                model: vec!["llama-test".to_string()],
            },
            optional: OptionalOllamaArgs {
                base_url: "http://localhost:11434".to_string(),
                logprobs: None,
                mirostat: None,
                mirostat_eta: None,
                mirostat_tau: None,
                num_ctx: None,
                num_gpu: None,
                num_gqa: None,
                num_predict: None,
                num_thread: None,
                repeat_last_n: None,
                repeat_penalty: None,
                seed: None,
                stop: None,
                temperature: None,
                tfs_z: None,
                top_k: None,
                top_p: None,
            },
        };

        assert!(config.build_model_options().is_none());
    }

    #[test]
    fn build_model_options_sets_selected_fields() {
        let options = make_config()
            .build_model_options()
            .expect("model options should be present");
        let value = serde_json::to_value(options).expect("options should serialize");

        assert_eq!(value["mirostat"], json!(2));
        assert_eq!(value["num_ctx"], json!(4096));
        assert_eq!(value["stop"], json!(["END"]));
        assert!(
            (value["top_p"]
                .as_f64()
                .expect("top_p should serialize as a number")
                - 0.9)
                .abs()
                < 1e-6
        );
    }
}
