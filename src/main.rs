mod cli;
mod consts;
mod evaluations;
mod protocol_types;
mod response_provider;
mod tui;
mod websockets;

use std::sync::Arc;

use clap::Parser;
use response_provider::{CustomProvider, OllamaProvider, OpenAIProvider, ResponseProvider};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args = cli::Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(Into::<tracing::Level>::into(cli_args.log_level))
        .init();

    let headers = cli_args.headers();

    let provider_command = match &cli_args.evaluation {
        cli::EvaluationCommand::SingleTurn { provider, .. }
        | cli::EvaluationCommand::MultiTurn { provider, .. } => provider,
    };

    let provider = match provider_command {
        cli::ProviderCommand::Ollama(config) => {
            Arc::new(OllamaProvider::new(config.clone(), &headers)?) as Arc<dyn ResponseProvider>
        }
        cli::ProviderCommand::OpenAI(config) => {
            Arc::new(OpenAIProvider::new(config.clone(), &headers)?) as Arc<dyn ResponseProvider>
        }
        cli::ProviderCommand::Custom(config) => {
            Arc::new(CustomProvider::new(&config, &headers)?) as Arc<dyn ResponseProvider>
        }
    };

    let websocket = websockets::connect(
        &cli_args.cbl_api_base_url,
        (&cli_args.evaluation).into(),
        &cli_args.cbl_api_key,
    )
    .await?;

    match cli_args.evaluation {
        cli::EvaluationCommand::SingleTurn { request, .. } => {
            let result = evaluations::singleturn::run_evaluation(websocket, provider, request)
                .await
                .map_err(|e| e.to_string())?;

            println!("Single-turn evaluation result: {result:?}");
        }
        cli::EvaluationCommand::MultiTurn { request, .. } => {
            let result = evaluations::multiturn::run_evaluation(websocket, provider, request, None) // TODO: pass in progress indicator channel
                .await
                .map_err(|e| e.to_string())?;

            println!("Multi-turn evaluation result: {result:?}");
        }
    }

    Ok(())
}
