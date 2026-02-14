mod cli;
mod completions;
mod consts;
mod evaluations;
mod protocol_types;
mod websockets;

use clap::Parser;

async fn run_single_turn_evaluation(request: protocol_types::SingleTurnRequest) {
    todo!("Implement single-turn evaluation logic");
}

async fn run_multi_turn_evaluation(request: protocol_types::MultiTurnRequest) {
    todo!("Implement multi-turn evaluation logic");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args = cli::Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(Into::<tracing::Level>::into(cli_args.log_level))
        .init();

    match cli_args.command {
        cli::EvalCommand::SingleTurn(request) => {
            run_single_turn_evaluation(request).await;
        }
        cli::EvalCommand::MultiTurn(request) => {
            run_multi_turn_evaluation(request).await;
        }
    }

    Ok(())
}
