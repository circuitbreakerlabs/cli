use std::path::PathBuf;

#[derive(Clone, Debug, clap::Args)]
pub struct CustomProviderConfig {
    /// Endpoint URL to POST to
    #[arg(long)]
    pub url: String,

    /// Path to the Rhai script file
    #[arg(long)]
    pub script: PathBuf,
}
