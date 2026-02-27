use super::headers::Headers;
use crate::protocol_types::{MultiTurnRequest, SingleTurnRequest};
use crate::response_provider::{CustomProviderConfig, OllamaProviderConfig, OpenAIProviderConfig};
use clap::{Parser, Subcommand, ValueEnum};
use reqwest::header::HeaderMap;

macro_rules! cyan_bold {
    ($s:expr) => {
        const_format::formatcp!("\x1b[1;36m{}\x1b[0m", $s)
    };
}

const ABOUT: &str = cyan_bold!("Circuit Breaker Labs CLI");

const LONG_ABOUT: &str = const_format::formatcp!(
    "{} {}

https://github.com/circuitbreakerlabs/cli
Protocol version {}",
    cyan_bold!("Circuit Breaker Labs CLI"),
    cyan_bold!(const_format::formatcp!("v{}", env!("CARGO_PKG_VERSION"))),
    crate::consts::version::PROTOCOL_VERSION
);

#[derive(Parser, Debug)]
#[command(
    version,
    about = ABOUT,
    long_about = LONG_ABOUT,
    arg_required_else_help = true
)]
pub struct Args {
    /// Circuit Breaker Labs API key
    #[arg(long, env = "CBL_API_KEY")]
    pub cbl_api_key: String,

    /// Circuit Breaker Labs API base URL
    #[arg(
        long,
        env = "CBL_API_BASE_URL",
        default_value = crate::consts::endpoints::CBL_BASE_URL
    )]
    pub cbl_api_base_url: String,

    /// Logging level
    #[arg(long, value_enum, default_value_t = LogLevel::Info)]
    pub log_level: LogLevel,

    /// Add custom headers to provider requests (format: "Key:Value", can be repeated)
    #[arg(long = "add-header", value_parser = clap::value_parser!(Headers))]
    headers: Vec<Headers>,

    #[command(subcommand)]
    pub evaluation: EvaluationCommand,
}

impl Args {
    pub fn headers(&self) -> HeaderMap {
        super::headers::merge_headers(&self.headers)
    }
}

#[derive(Subcommand, Debug)]
pub enum EvaluationCommand {
    /// Run single-turn evaluation
    SingleTurn {
        #[command(subcommand)]
        provider: ProviderCommand,
        #[command(flatten)]
        request: SingleTurnRequest,
    },

    /// Run multi-turn evaluation
    MultiTurn {
        #[command(subcommand)]
        provider: ProviderCommand,
        #[command(flatten)]
        request: MultiTurnRequest,
    },
}

#[derive(Subcommand, Debug)]
#[command(rename_all = "lowercase")]
pub enum ProviderCommand {
    /// Use Ollama provider
    Ollama(OllamaProviderConfig),
    /// Use OpenAI provider
    OpenAI(OpenAIProviderConfig),
    /// Use Custom Rhai-scripted provider
    Custom(CustomProviderConfig),
}

#[derive(Clone, Copy, ValueEnum, Debug)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<LogLevel> for tracing::Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => tracing::Level::ERROR,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Trace => tracing::Level::TRACE,
        }
    }
}
