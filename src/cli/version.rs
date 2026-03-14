const GIT_BRANCH: &str = match option_env!("VERGEN_GIT_BRANCH") {
    Some(branch) => branch,
    None => "unknown",
};

const GIT_COMMIT: &str = match option_env!("VERGEN_GIT_SHA") {
    Some(commit) => commit,
    None => "unknown",
};

const GIT_TAG: &str = match option_env!("CBL_GIT_TAG") {
    Some(tag) => tag,
    None => "unknown",
};

pub const VERSION: &str = const_format::formatcp!(
    "v{}
protocol v{}
built from branch '{}' at commit {} tagged {}",
    env!("CARGO_PKG_VERSION"),
    crate::consts::version::PROTOCOL_VERSION,
    GIT_BRANCH,
    GIT_COMMIT,
    GIT_TAG
);

#[cfg(test)]
mod tests {
    use super::super::Args;
    use clap::{CommandFactory, Parser, error::ErrorKind};

    #[test]
    fn displays_rich_version_output() {
        let err = Args::try_parse_from(["cbl", "--version"])
            .expect_err("version flag should short-circuit parsing");

        assert_eq!(err.kind(), ErrorKind::DisplayVersion);

        let rendered = err.to_string();
        assert!(rendered.contains(env!("CARGO_PKG_NAME")));
        assert!(rendered.contains(env!("CARGO_PKG_VERSION")));
        assert!(rendered.contains(crate::consts::version::PROTOCOL_VERSION));
        assert!(rendered.contains("built from branch"));
        assert!(rendered.contains("at commit"));
        assert!(rendered.contains("tagged"));
    }

    #[test]
    fn clap_command_version_matches_expected_format() {
        let rendered = Args::command().render_version().clone();

        assert!(rendered.contains(env!("CARGO_PKG_NAME")));
        assert!(rendered.contains(env!("CARGO_PKG_VERSION")));
        assert!(rendered.contains(crate::consts::version::PROTOCOL_VERSION));
        assert!(rendered.contains("built from branch"));
        assert!(rendered.contains("at commit"));
        assert!(rendered.contains("tagged"));
    }
}
