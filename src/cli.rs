use crate::protocol_types::{MultiTurnRequest, SingleTurnRequest};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(about = "Circuit Breaker Labs CLI", long_about = None)]
pub struct Args {
    /// Circuit Breaker Labs API key
    #[arg(long, env = "CBL_API_KEY")]
    pub cbl_api_key: String,

    /// Completion endpoint URL
    #[arg(long, env = "COMPLETION_ENDPOINT")]
    pub endpoint: String,

    /// Input/output protocol shape for the completion endpoint
    #[arg(long, value_enum, default_value_t = ProtocolShape::Ollama)]
    pub protocol: ProtocolShape,

    /// Circuit Breaker Labs API base URL
    #[arg(
        long,
        env = "CBL_API_BASE_URL",
        default_value = "https://api.circuitbreakerlabs.ai/v1/"
    )]
    pub cbl_api_base_url: String,

    /// Logging level
    #[arg(long, value_enum, default_value_t = LogLevel::Info)]
    pub log_level: LogLevel,

    #[command(subcommand)]
    pub command: EvalCommand,
}

#[derive(Clone, ValueEnum, Debug)]
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

#[derive(Clone, ValueEnum, Debug)]
#[value(rename_all = "lowercase")]
pub enum ProtocolShape {
    Ollama,
    OpenAI,
    Anthropic,
}

#[derive(Subcommand, Debug)]
pub enum EvalCommand {
    /// Run single-turn evaluation
    SingleTurn(SingleTurnRequest),
    /// Run multi-turn evaluation
    MultiTurn(MultiTurnRequest),
}
