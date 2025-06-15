use crate::db::{insert_derivation_hash, insert_system_name};

use anyhow::{Context, Result};
use futures::future::join_all;
use futures::stream::iter;
use futures::stream::{self, StreamExt};
use serde_json::Value;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::{Duration, timeout};
use tracing::{debug, error, info, trace, warn};
/// Parses a Nix flake and extracts the defined NixOS configuration names.
///
/// # Arguments
///
/// * `repo_url` - A string containing the path or URL to a Nix flake.
///
/// # Returns
///
/// * A `Result<()>` that logs the list of NixOS system names defined in the flake.
///
/// # Errors
///
/// Returns an error if the `nix flake show` command fails or if the output
/// cannot be parsed as valid JSON.
pub async fn get_nixos_configurations(repo_url: String) -> anyhow::Result<Vec<String>> {
    let flake_show = Command::new("nix")
        .args(["flake", "show", "--json", &repo_url])
        .output()
        .await?;
    let flake_json: serde_json::Value = serde_json::from_slice(&flake_show.stdout)?;

    let nixos_configs = flake_json["nixosConfigurations"]
        .as_object()
        .context("missing nixosConfigurations")?
        .keys()
        .cloned()
        .collect::<Vec<String>>();

    info!("nixosConfigurations: {:?}", nixos_configs);
    Ok(nixos_configs)
}

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
pub async fn get_nixos_configurations_at_commit(
    repo_url: &str,
    commit: &str,
) -> Result<Vec<String>> {
    debug!("🔍 get_nixos_configurations_at_commit called with repo_url={repo_url} commit={commit}");
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

    debug!("About to do `nix flake show --json`");
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

/// Returns the derivation output path (hash) for a specific NixOS system from a given flake.
///
/// If the flake is a local path, the given commit will be checked out via `git`.
///
/// # Arguments
/// * `system` - The nixosConfigurations.<system> to target
/// * `flake_url` - Local path or remote git URL
/// * `commit` - Git commit to use if applicable
///
/// # Returns
/// * A string like `/nix/store/<hash>-toplevel`
pub async fn get_system_derivation(
    system: &str,
    flake_url: &str,
    commit: &str,
) -> std::result::Result<String, anyhow::Error> {
    debug!("🔍 Determining derivation for system: {system}, flake: {flake_url}, commit: {commit}");

    let is_path = Path::new(flake_url).exists();
    debug!("📁 Is local path: {is_path}");

    if is_path {
        debug!("📌 Running 'git checkout {commit}' in {flake_url}");
        let status = Command::new("git")
            .args(["-C", flake_url, "checkout", commit])
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!(
                "❌ git checkout failed for path {} at rev {}",
                flake_url,
                commit
            );
        }
    }

    let flake_target = if is_path {
        format!("{flake_url}#nixosConfigurations.{system}.config.system.build.toplevel")
    } else if flake_url.starts_with("git+") {
        format!(
            "{flake_url}?rev={commit}#nixosConfigurations.{system}.config.system.build.toplevel"
        )
    } else {
        format!(
            "git+{flake_url}?rev={commit}#nixosConfigurations.{system}.config.system.build.toplevel"
        )
    };

    debug!("🔨 Building flake target: {flake_target} (dry-run)");

    let output = Command::new("/run/current-system/sw/bin/nix")
        .args(["build", &flake_target, "--dry-run", "--json"])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("❌ nix build failed: {}", stderr.trim());
        anyhow::bail!("nix build failed: {}", stderr.trim());
    }

    let parsed: Value = serde_json::from_slice(&output.stdout)?;
    let hash = parsed
        .get(0)
        .and_then(|v| v["outputs"]["out"].as_str())
        .context("❌ Missing derivation output in nix build output")?
        .to_string();

    debug!("✅ Derivation path for {system}: {hash}");
    Ok(hash)
}

/// Fetches derivation output paths for all provided NixOS system configurations at a specific commit.
///
/// # Arguments
///
/// * `systems` - A list of NixOS system names (e.g., `"x86_64-linux"`, `"aarch64-linux"`).
/// * `flake_url` - A Nix flake reference (local path or remote, e.g., `git+https://...`).
/// * `commit_hash` - The Git commit to evaluate the flake at.
///
/// # Returns
///
/// * A `Result` containing a list of tuples, where each tuple includes the system name and
///   its corresponding derivation output path.
///
/// # Errors
///
/// Returns an error if any derivation fails to resolve or if the Nix command fails.
pub async fn get_all_derivations(
    systems: Vec<String>,
    flake_url: &str,
    commit_hash: &str,
) -> Result<Vec<(String, String)>> {
    let tasks = systems.into_iter().map(|system| {
        let path = flake_url.to_string();
        let commit = commit_hash.to_string();
        async move {
            let hash = get_system_derivation(&system, &path, &commit).await?;
            Ok((system, hash)) as Result<_>
        }
    });

    let results = futures::future::join_all(tasks).await;
    results.into_iter().collect()
}

/// Streams derivation output hashes for each NixOS system configuration in parallel,
/// calling `handle_result` as each one completes.
///
/// # Arguments
///
/// * `systems` - List of NixOS system names to evaluate.
/// * `flake_path` - Path or URL of the flake.
/// * `commit_hash` - Git commit to use for evaluation.
/// * `handle_result` - Async callback invoked with each `(system, hash)` pair.
///
/// # Returns
///
/// * `Ok(())` if all derivations are handled successfully, or early logs on failure.
pub async fn stream_derivations(
    systems: Vec<String>,
    flake_path: &str,
    commit_hash: &str,
    insert_system_fn: Arc<
        dyn Fn(String, String, String) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
            + Send
            + Sync,
    >,
    handle_result: Arc<
        Mutex<dyn FnMut(String, String) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send>,
    >,
) -> Result<()> {
    info!(
        "🌀 starting stream_derivations for {} systems at commit {}",
        systems.len(),
        commit_hash
    );

    // insert all systems into db
    for system in &systems {
        insert_system_name(commit_hash, flake_path, system).await?;
    }

    let path = flake_path.to_string();
    let commit = commit_hash.to_string();
    let insert_system_fn = insert_system_fn.clone();

    let systems_with_insert = futures::future::join_all(systems.into_iter().map(|system| {
        let insert_system_fn = insert_system_fn.clone();
        let repo_url = path.clone();
        let commit = commit.clone();
        async move {
            insert_system_fn(commit.clone(), repo_url.clone(), system.clone()).await?;
            Ok(system)
        }
    }))
    .await
    .into_iter()
    .collect::<Result<Vec<_>>>()?;

    debug!("✅ systems_with_insert: {:?}", systems_with_insert);

    stream::iter(systems_with_insert)
        .for_each_concurrent(8, {
            let path = path.clone();
            let commit = commit.clone();
            let handle_result = handle_result.clone();

            move |system: String| {
                let path = path.clone();
                let commit = commit.clone();
                let handle_result = handle_result.clone();

                async move {
                    debug!("🔧 fetching derivation for system: {}", system);
                    match get_system_derivation(&system, &path, &commit).await {
                        Ok(hash) => {
                            debug!("📦 got derivation: {} => {}", system, hash);
                            let hash_ref = &hash;
                            if let Err(e) =
                                handle_result.lock().await(system.clone(), hash.clone()).await
                            {
                                error!("❌ handler failed for {}: {:?}", system, e);
                            }
                            if let Err(e) =
                                insert_derivation_hash(&commit, &path, &system, hash_ref).await
                            {
                                error!(
                                    "❌ failed to insert derivation hash for {}: {:?}",
                                    system, e
                                );
                            }
                        }
                        Err(e) => {
                            error!("❌ failed to get derivation for {}: {:?}", system, e);
                        }
                    }
                }
            }
        })
        .await;

    info!("🎯 finished stream_derivations for commit {}", commit_hash);
    Ok(())
}
