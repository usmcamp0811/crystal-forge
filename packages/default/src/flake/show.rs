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
// pub async fn get_all_derivations(
//     systems: Vec<String>,
//     flake_url: &str,
//     commit_hash: &str,
// ) -> Result<Vec<(String, String)>> {
//     let tasks = systems.into_iter().map(|system| {
//         let path = flake_url.to_string();
//         let commit = commit_hash.to_string();
//         async move {
//             let hash = get_system_derivation(&system, &path, &commit).await?;
//             Ok((system, hash)) as Result<_>
//         }
//     });
//
//     let results = futures::future::join_all(tasks).await;
//     results.into_iter().collect()
// }
