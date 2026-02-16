use clap;
use ollama_rs::models::ModelOptions;

#[derive(Clone, Debug, clap::Args)]
#[clap(next_help_heading = "Required Options")]
pub struct RequiredOllamaArgs {
    /// Ollama model name
    #[arg(long)]
    pub model: String,
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
