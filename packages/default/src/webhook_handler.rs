//! This module handles incoming webhooks, triggering the fetching of NixOS
//! configurations and streaming derivations to process them. It uses injected
//! async functions for persistence and derivation processing.

use crate::db::{insert_commit, insert_derivation_hash, insert_system_name};
use crate::flake_watcher::{get_nixos_configurations, stream_derivations};
use axum::{Json, http::StatusCode};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// A boxed async handler for inserting a derivation hash.
///
/// This closure is expected to take the system name and derivation hash,
/// and insert them into persistent storage.
pub type BoxedHandler = Box<
    dyn Fn(String, String) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
        + Send
        + Sync,
>;

/// Handles an incoming webhook request for a Git push or merge event.
///
/// Extracts the repo URL and commit hash from the payload, stores the commit,
/// fetches the NixOS system configurations for the repo, and begins async
/// processing of derivation hashes for those configurations.
///
/// # Parameters
/// - `payload`: The webhook body as parsed JSON.
/// - `insert_commit_fn`: Async function that inserts a commit into the database.
/// - `get_configs_fn`: Async function that retrieves a list of NixOS system configurations from the repo.
/// - `stream_fn`: Async function that evaluates derivations and handles each result.
/// - `insert_deriv_hash_fn`: Async function that stores derivation hashes.
///
/// # Returns
/// - [`StatusCode::ACCEPTED`] if the webhook was valid and processing was started.
/// - [`StatusCode::BAD_REQUEST`] if required fields were missing.
/// - [`StatusCode::INTERNAL_SERVER_ERROR`] if a DB or config operation failed.

pub async fn webhook_handler<F1, F2, F3, F4>(
    payload: Value,
    insert_commit_fn: F1,
    get_configs_fn: F2,
    stream_fn: F3,
    insert_deriv_hash_fn: F4,
) -> StatusCode
where
    F1: Fn(&str, &str) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
        + Send
        + Sync
        + 'static,
    F2: Fn(String) -> Pin<Box<dyn Future<Output = Result<Vec<String>, anyhow::Error>> + Send>>
        + Send
        + Sync
        + Clone
        + 'static,
    F3: Fn(
            Vec<String>,
            &str,
            Arc<Mutex<BoxedHandler>>,
        ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
        + Send
        + Sync
        + Clone
        + 'static,
    F4: Fn(
            String,
            String,
            String,
            String,
        ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
        + Send
        + Sync
        + Clone
        + 'static,
{
    info!("📩 Received webhook payload");

    let Some(repo_url) = payload
        .pointer("/repository/clone_url")
        .or_else(|| payload.pointer("/project/web_url"))
        .and_then(|v| v.as_str())
        .map(String::from)
    else {
        warn!("⚠️ Could not extract repository URL from payload");
        return StatusCode::BAD_REQUEST;
    };

    let Some(commit_hash) = payload
        .pointer("/after")
        .or_else(|| payload.pointer("/checkout_sha"))
        .and_then(|v| v.as_str())
        .map(String::from)
    else {
        warn!("⚠️ Could not extract commit hash from payload");
        return StatusCode::BAD_REQUEST;
    };

    info!("🔗 Repo: {repo_url} @ {commit_hash}");

    // Do lightweight validation up front, spawn the rest
    let insert_commit_fn = Arc::new(insert_commit_fn);
    let get_configs_fn = Arc::new(get_configs_fn);
    let stream_fn = Arc::new(stream_fn);
    let insert_deriv_hash_fn = Arc::new(insert_deriv_hash_fn);

    tokio::spawn({
        let repo_url = repo_url.clone();
        let commit_hash = commit_hash.clone();
        let insert_commit_fn = Arc::clone(&insert_commit_fn);
        let get_configs_fn = Arc::clone(&get_configs_fn);
        let stream_fn = Arc::clone(&stream_fn);
        let insert_deriv_hash_fn = Arc::clone(&insert_deriv_hash_fn);

        async move {
            if let Err(e) = insert_commit_fn(&commit_hash, &repo_url).await {
                error!("❌ Failed to insert commit: {e:?}");
                return;
            }

            let configs = match get_configs_fn(repo_url.clone()).await {
                Ok(c) => {
                    debug!("📦 Retrieved configurations: {:?}", c);
                    c
                }
                Err(err) => {
                    error!("❌ Failed to get configurations: {err:?}");
                    return;
                }
            };

            // commit_hash and repo_url are swapped i need to find where they got swapped
            for system_name in &configs {
                if let Err(e) = insert_system_name(&commit_hash, &repo_url, system_name).await {
                    error!("❌ Failed to insert system name {system_name}: {e:?}");
                    return;
                }
            }

            let repo_url_outer = repo_url.clone();
            let commit_hash_outer = commit_hash.clone();

            let repo_url_for_closure = repo_url_outer.clone();
            let repo_url_for_stream = repo_url_for_closure.clone();
            let repo_url_for_log = repo_url_outer.clone();

            let handle_result: Arc<Mutex<BoxedHandler>> =
                Arc::new(Mutex::new(Box::new(move |system: String, hash: String| {
                    let repo_url = repo_url_for_closure.clone();
                    let commit_hash = commit_hash_outer.clone();
                    debug!("📝 Handling derivation: system={system}, hash={hash}");
                    Box::pin(insert_deriv_hash_fn(system, hash, repo_url, commit_hash))
                })));

            if let Err(e) = stream_fn(configs, &repo_url_for_stream, handle_result).await {
                error!("❌ stream_fn failed: {:?}", e);
            } else {
                info!("✅ Finished streaming derivations for {repo_url_for_log}");
            }
        }
    });

    StatusCode::ACCEPTED
}
