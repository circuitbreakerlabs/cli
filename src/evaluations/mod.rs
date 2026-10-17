mod engine;
pub mod err;
pub mod multiturn;
pub mod singleturn;
#[cfg(test)]
mod test_support;

pub use err::EvaluationError;

#[derive(Clone, Debug)]
pub enum EvaluationType {
    Voice,
    VoiceRerun,
    SingleTurnVoice,
    SingleTurnVoiceRerun,
    SingleTurn,
    SingleTurnRerun,
    MultiTurn,
    MultiTurnRerun,
}

#[cfg(eval_targets)]
impl From<&crate::cli::EvaluationCommand> for EvaluationType {
    fn from(cmd: &crate::cli::EvaluationCommand) -> Self {
        match cmd {
            crate::cli::EvaluationCommand::SingleTurn { provider, .. } => {
                if provider.is_voice() {
                    EvaluationType::SingleTurnVoice
                } else {
                    EvaluationType::SingleTurn
                }
            }
            crate::cli::EvaluationCommand::MultiTurn { provider, .. } => {
                if provider.is_voice() {
                    EvaluationType::Voice
                } else {
                    EvaluationType::MultiTurn
                }
            }
            crate::cli::EvaluationCommand::ReRun { rerun } => match rerun {
                crate::cli::ReRunEvaluationCommand::SingleTurn { provider, .. } => {
                    if provider.is_voice() {
                        EvaluationType::SingleTurnVoiceRerun
                    } else {
                        EvaluationType::SingleTurnRerun
                    }
                }
                crate::cli::ReRunEvaluationCommand::MultiTurn { provider, .. } => {
                    if provider.is_voice() {
                        EvaluationType::VoiceRerun
                    } else {
                        EvaluationType::MultiTurnRerun
                    }
                }
            },
        }
    }
}

#[cfg(all(test, eval_targets))]
mod routing_tests {
    use super::EvaluationType;
    use crate::cli::{Args, Command};
    use clap::Parser;

    #[test]
    fn selected_target_determines_endpoint() {
        let targets: &[(&str, &[&str], bool)] = &[
            #[cfg(feature = "openai")]
            ("openai", &["--api-key", "unused", "--model", "test"], false),
            #[cfg(feature = "ollama")]
            ("ollama", &["--model", "test"], false),
            #[cfg(feature = "custom")]
            (
                "custom",
                &["--script", "unused.rhai", "--url", "http://localhost"],
                false,
            ),
            #[cfg(feature = "livekit")]
            ("livekit", &["--config", "unused.toml"], true),
        ];
        for &(target, options, voice) in targets {
            for (single, rerun) in [(true, false), (false, false), (false, true), (true, true)] {
                let mut argv = vec!["cbl", "--cbl-api-key", "unused", "eval"];
                if rerun {
                    argv.push("re-run");
                }
                argv.push(if single { "single-turn" } else { "multi-turn" });
                argv.extend(["--threshold", "0.5"]);
                if single {
                    argv.extend(["--variations", "2", "--maximum-iteration-layers", "1"]);
                } else {
                    argv.extend(["--max-turns", "4"]);
                }
                argv.extend(if rerun {
                    ["--evaluation-id", "123"]
                } else {
                    ["--test-case-groups", "test"]
                });
                argv.push(target);
                argv.extend(options);
                let parsed =
                    Args::try_parse_from(&argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
                parsed.validate().unwrap();
                let Some(Command::Eval { evaluation }) = parsed.command else {
                    panic!("expected evaluation")
                };
                let kind = EvaluationType::from(&evaluation);
                let endpoint = crate::consts::endpoints::endpoint_from_evaluation_type(&kind);
                let expected = match (voice, single, rerun) {
                    (true, true, false) => "/ws/singleturn_voice_evaluation",
                    (true, false, false) => "/ws/multiturn_voice_evaluation",
                    (true, false, true) => "/ws/multiturn_voice_rerun_evaluation",
                    (false, true, false) => "/ws/singleturn_evaluation",
                    (false, true, true) => "/ws/singleturn_rerun_evaluation",
                    (false, false, false) => "/ws/multiturn_evaluation",
                    (false, false, true) => "/ws/multiturn_rerun_evaluation",
                    (true, true, true) => "/ws/singleturn_voice_rerun_evaluation",
                };
                assert_eq!(endpoint, expected, "{argv:?}");
            }
        }
    }
}
