//! Exercise the public CLI surface for every feature configuration.
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cbl"))
        .env_remove("CBL_API_KEY")
        .args([
            "--cbl-api-key",
            "unused",
            "--cbl-api-base-url",
            "invalid-url",
        ])
        .args(args)
        .output()
        .unwrap()
}

fn help(args: &[&str]) -> String {
    let output = cli(args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn command_surface_matches_enabled_targets() {
    let top = help(&["--help"]);
    assert!(
        top.lines()
            .any(|line| line.trim_start().starts_with("api "))
    );
    assert_eq!(
        top.lines()
            .any(|line| line.trim_start().starts_with("eval ")),
        cfg!(eval_targets)
    );
    assert!(help(&["api", "--help"]).contains("evaluations"));

    if !cfg!(eval_targets) {
        let output = cli(&["eval"]);
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand 'eval'"));
        return;
    }

    for path in [
        vec!["eval", "single-turn"],
        vec!["eval", "multi-turn"],
        vec!["eval", "re-run", "multi-turn"],
        vec!["eval", "re-run", "single-turn"],
    ] {
        let mut args = path.clone();
        args.push("--help");
        let output = help(&args);
        assert!(!output.contains("--voice"));
        for (name, enabled) in [
            ("openai", cfg!(feature = "openai")),
            ("ollama", cfg!(feature = "ollama")),
            ("custom", cfg!(feature = "custom")),
            ("livekit", cfg!(feature = "livekit")),
        ] {
            assert_eq!(
                output
                    .lines()
                    .any(|line| line.split_whitespace().next() == Some(name)),
                enabled,
                "{path:?}: {name}"
            );
            let mut target_args = path.clone();
            target_args.extend([name, "--help"]);
            let target = cli(&target_args);
            assert_eq!(
                target.status.success(),
                enabled,
                "{target_args:?}: {}",
                String::from_utf8_lossy(&target.stderr)
            );
            if !enabled {
                assert_eq!(target.status.code(), Some(2));
                assert!(
                    String::from_utf8_lossy(&target.stderr).contains("unrecognized subcommand")
                );
            }
        }
    }
}

#[cfg(eval_targets)]
#[test]
fn removed_voice_option_is_rejected_before_network_access() {
    let output = cli(&["eval", "multi-turn", "--voice"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unexpected argument '--voice'"));
}

#[cfg(feature = "livekit")]
#[test]
fn livekit_preparation_precedes_api_connection() {
    let output = cli(&[
        "eval",
        "multi-turn",
        "--threshold",
        "0.75",
        "--max-turns",
        "4",
        "--test-case-groups",
        "test",
        "livekit",
        "--config",
        "/missing/voice.toml",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read target file"));
}
