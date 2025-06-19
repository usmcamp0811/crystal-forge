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
pub async fn evaluate_derivations(
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
                                update_derivation_hash(&commit, &path, &system, hash_ref).await
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
