#[derive(Clone, Debug, clap::Args)]
pub struct OpenAIProviderConfig {
    /// Completion endpoint URL
    #[arg(long, env = "COMPLETION_ENDPOINT")]
    endpoint: String,

    /// OpenAI organization ID
    #[arg(long, env = "OPENAI_ORG_ID")]
    org_id: Option<String>,

    /// OpenAI parameter 1
    #[arg(long)]
    parameter_1: String,
}
