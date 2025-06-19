use crate::db::{insert_derivation_hash, insert_system_name};
use anyhow::{Context, Result};
use futures::future::join_all;
use futures::stream::{self, StreamExt, iter};
use serde_json::Value;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::io::AsyncWriteExt;
use tokio::process::Command as TokioCommand;
use tokio::sync::Mutex;
use tokio::time::{Duration, timeout};
use tracing::{debug, error, info, trace, warn};

/// Returns the list of NixOS configurations defined in a flake at a given Git commit.
///
/// If `repo_url` is a local filesystem path, this will attempt to run
/// `git checkout <commit>` inside that path before evaluating the flake.
///
/// # Arguments
/// - `repo_url`: Git URL or filesystem path to a Nix flake
/// - `commit`: Git commit hash to evaluate the flake at
///
/// # Returns
/// A list of configuration names under `nixosConfigurations` in the flake.
///
/// # Errors
/// Returns an error if:
/// - `git checkout` fails (for local paths)
/// - `nix flake show` fails
/// - `nixosConfigurations` is missing from the flake output
pub async fn list_nixos_configurations_at_commit(
    repo_url: &str,
    commit: &str,
) -> Result<Vec<String>> {
    debug!(
        "🔍 list_nixos_configurations_at_commit called with repo_url={repo_url} commit={commit}"
    );
    // Determine if the repo URL is a local path
    let is_path = Path::new(repo_url).exists();

    // Build the flake URI accordingly
    let flake_uri = if is_path {
        // Local path — use as-is
        repo_url.to_string()
    } else if repo_url.starts_with("git+") {
        // Already prefixed — just add the rev
        format!("{}?rev={}", repo_url, commit)
    } else {
        // Assume it's a plain git URL — add both prefix and rev
        format!("git+{}?rev={}", repo_url, commit)
    };

    // If it's a local path, perform a git checkout at the requested rev
    if is_path {
        let status = Command::new("git")
            .args(["-C", repo_url, "checkout", commit])
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!(
                "git checkout failed for path {} at rev {}",
                repo_url,
                commit
            );
        }
    }

    // Run `nix flake show` on the constructed flake URI
    let output = timeout(
        Duration::from_secs(30),
        Command::new("nix")
            .args(["flake", "show", "--json", &flake_uri])
            .output(),
    )
    .await??; // unwrap timeout then result

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("❌ nix flake show failed for {flake_uri}: {stderr}");
        anyhow::bail!("nix flake show failed: {}", stderr.trim());
    }

    debug!("✅ nix flake show completed");
    // Parse the output JSON
    let flake_json: serde_json::Value = serde_json::from_slice(&output.stdout)?;

    // Extract nixosConfigurations keys
    let nixos_configs = flake_json["nixosConfigurations"]
        .as_object()
        .context("missing nixosConfigurations")?
        .keys()
        .cloned()
        .collect::<Vec<String>>();

    debug!("nixosConfigurations: {:?}", nixos_configs);
    Ok(nixos_configs)
}
