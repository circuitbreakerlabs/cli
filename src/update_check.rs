use ratatui::crossterm::style::{Attribute, Color, SetAttribute, SetForegroundColor};
use std::time::Duration;
use update_informer::{Check, registry};

const GITHUB_RELEASES_URL: &str = "https://github.com/circuitbreakerlabs/cli/releases/latest";
const GITHUB_RELEASE_CHECK_INTERVAL: Duration = Duration::hours(24);

pub async fn print_update_warning_if_needed(log_mode: bool) {
    tracing::info!("Checking for update...");
    let latest_version = tokio::task::spawn_blocking(check_for_github_release_update)
        .await
        .ok()
        .flatten();

    if let Some(latest_version) = latest_version {
        let latest_version = latest_version.to_string();
        print_update_warning(log_mode, &latest_version);
    }
}

fn check_for_github_release_update() -> Option<update_informer::Version> {
    let repository = github_repository_name()?;
    let informer = update_informer::new(registry::GitHub, repository, env!("CARGO_PKG_VERSION"))
        .interval(GITHUB_RELEASE_CHECK_INTERVAL);

    informer.check_version().ok().flatten()
}

fn github_repository_name() -> Option<&'static str> {
    let repository = env!("CARGO_PKG_REPOSITORY")
        .trim_end_matches('/')
        .trim_end_matches(".git");

    repository
        .strip_prefix("https://github.com/")
        .or_else(|| repository.strip_prefix("http://github.com/"))
        .or_else(|| repository.strip_prefix("git@github.com:"))
}

fn print_update_warning(log_mode: bool, latest_version: &str) {
    let current_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let message = format!(
        "A newer release is available: {current_version} -> {latest_version}. Download it from {GITHUB_RELEASES_URL}",
    );

    if log_mode {
        tracing::warn!("{message}");
    } else {
        println!(
            "{}{}Warning:{}{} {}",
            SetForegroundColor(Color::Yellow),
            SetAttribute(Attribute::Bold),
            SetAttribute(Attribute::Reset),
            SetForegroundColor(Color::Reset),
            message,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{GITHUB_RELEASES_URL, github_repository_name};

    #[test]
    fn parses_github_repository_name_from_package_metadata() {
        assert_eq!(github_repository_name(), Some("circuitbreakerlabs/cli"));
    }

    #[test]
    fn release_url_points_to_latest_github_release() {
        assert_eq!(
            GITHUB_RELEASES_URL,
            "https://github.com/circuitbreakerlabs/cli/releases/latest"
        );
    }
}
