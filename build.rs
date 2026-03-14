use anyhow::{Result, anyhow};
use std::collections::BTreeMap;
use std::process::Command;
use vergen::{AddCustomEntries, CargoRerunIfChanged, CargoWarning, DefaultConfig};
use vergen_gitcl::{Emitter, Gitcl};

const EXACT_TAG_ENV: &str = "CBL_GIT_TAG";
const UNKNOWN_VALUE: &str = "unknown";

#[derive(Default)]
struct ExactTag;

impl AddCustomEntries<&'static str, String> for ExactTag {
    fn add_calculated_entries(
        &self,
        _idempotent: bool,
        cargo_rustc_env_map: &mut BTreeMap<&'static str, String>,
        cargo_rerun_if_changed: &mut CargoRerunIfChanged,
        _cargo_warning: &mut CargoWarning,
    ) -> Result<()> {
        cargo_rerun_if_changed.push(".git/packed-refs".to_string());
        cargo_rerun_if_changed.push(".git/refs/tags".to_string());

        let output = Command::new("git")
            .args(["tag", "--points-at", "HEAD", "--sort=-creatordate"])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!("git tag --points-at HEAD failed"));
        }

        let tag = String::from_utf8(output.stdout)?
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or(UNKNOWN_VALUE)
            .to_string();

        cargo_rustc_env_map.insert(EXACT_TAG_ENV, tag);

        Ok(())
    }

    fn add_default_entries(
        &self,
        config: &DefaultConfig,
        cargo_rustc_env_map: &mut BTreeMap<&'static str, String>,
        cargo_rerun_if_changed: &mut CargoRerunIfChanged,
        cargo_warning: &mut CargoWarning,
    ) -> Result<()> {
        cargo_rerun_if_changed.push(".git/packed-refs".to_string());
        cargo_rerun_if_changed.push(".git/refs/tags".to_string());

        if *config.fail_on_error() {
            Err(anyhow!(config.error().to_string()))
        } else {
            cargo_rustc_env_map.insert(EXACT_TAG_ENV, UNKNOWN_VALUE.to_string());
            cargo_warning.push(format!(
                "failed to determine exact git tag for HEAD: {}",
                config.error()
            ));
            Ok(())
        }
    }
}

fn main() -> Result<()> {
    let gitcl = Gitcl::builder().branch(true).sha(true).build();

    Emitter::default()
        .add_instructions(&gitcl)?
        .add_custom_instructions(&ExactTag)?
        .emit()?;

    Ok(())
}
