use std::process::Command;

fn check_help(path: &[&str], flag: &str, env_name: &str, description: &str) {
    for (secret, masked) in [
        (None, ""),
        (Some(""), "****"),
        (Some("xyz"), "****"),
        (Some("wxyz"), "****"),
        (Some("help-test-secret-1234"), "****1234"),
        (Some("help-test-secret-aé🔑字"), "****aé🔑字"),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cbl"));
        command
            .env_remove("CBL_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .env("NO_COLOR", "1");
        if let Some(secret) = secret {
            command.env(env_name, secret);
        }
        let output = command.args(path).arg(flag).output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(output.status.success(), "{path:?} {flag}: {stderr}");
        assert!(stdout.contains(description), "{stdout}");
        assert!(
            stdout.contains(&format!("[env: {env_name}={masked}]")),
            "{stdout}"
        );
        if let Some(secret) = secret.filter(|value| !value.is_empty()) {
            assert!(!stdout.contains(secret), "secret leaked to stdout");
            assert!(!stderr.contains(secret), "secret leaked to stderr");
        }
    }
}

#[test]
fn cbl_help_masks_api_key() {
    for flag in ["--help", "-h"] {
        check_help(&[], flag, "CBL_API_KEY", "Circuit Breaker Labs API key");
    }
}

#[cfg(feature = "openai")]
#[test]
fn openai_help_masks_api_key_for_all_evaluation_paths() {
    for path in [
        vec!["eval", "single-turn", "openai"],
        vec!["eval", "multi-turn", "openai"],
        vec!["eval", "re-run", "single-turn", "openai"],
        vec!["eval", "re-run", "multi-turn", "openai"],
    ] {
        for flag in ["--help", "-h"] {
            check_help(&path, flag, "OPENAI_API_KEY", "OpenAI API key");
        }
    }
}
