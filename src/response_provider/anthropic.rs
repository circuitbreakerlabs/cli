#[derive(Clone, Debug, clap::Args)]
pub struct AnthropicProviderConfig {
    /// Anthropic beta features
    #[arg(long)]
    beta: Option<String>,

    /// Anthropic parameter 1
    #[arg(long)]
    parameter_1: String,
}
