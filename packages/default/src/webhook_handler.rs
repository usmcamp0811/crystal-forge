//! This module handles incoming webhooks, triggering the fetching of NixOS
//! configurations and streaming derivations to process them. It uses injected
//! async functions for persistence and derivation processing.

use crate::db::{insert_commit, insert_derivation_hash};
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
    F3: Fn(Vec<String>, &str, Arc<Mutex<BoxedHandler>>) -> Pin<Box<dyn Future<Output = ()> + Send>>
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

    // Extract repo URL from payload
    let Some(repo_url) = payload
        .pointer("/repository/clone_url")
        .or_else(|| payload.pointer("/project/web_url"))
        .and_then(|v| v.as_str())
        .map(String::from)
    else {
        warn!("⚠️ Could not extract repository URL from payload");
        return StatusCode::BAD_REQUEST;
    };

    // Extract commit hash from payload
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

    // Insert the commit record
    if let Err(e) = insert_commit_fn(&commit_hash, &repo_url).await {
        error!("❌ Failed to insert commit: {e:?}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    // Fetch NixOS configurations
    let configs = match get_configs_fn(repo_url.clone()).await {
        Ok(c) => {
            debug!("📦 Retrieved configurations: {:?}", c);
            c
        }
        Err(err) => {
            error!("❌ Failed to get configurations: {err:?}");
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    };

    // Spawn background task for derivation streaming
    tokio::spawn({
        let repo_url_outer = repo_url.clone();
        let commit_hash_outer = commit_hash.clone();
        let stream_fn = stream_fn.clone();
        let insert_deriv_hash_fn = insert_deriv_hash_fn.clone();
        let configs = configs.clone();

        async move {
            info!("🔧 Starting derivation stream for: {repo_url_outer}");

            let repo_url_for_closure = repo_url_outer.clone();
            let handle_result: Arc<Mutex<BoxedHandler>> =
                Arc::new(Mutex::new(Box::new(move |system: String, hash: String| {
                    let repo_url = repo_url_for_closure.clone();
                    let commit_hash = commit_hash_outer.clone();
                    debug!("📝 Handling derivation: system={system}, hash={hash}");
                    Box::pin(insert_deriv_hash_fn(commit_hash, repo_url, system, hash))
                })));

            stream_fn(configs, &repo_url_outer, handle_result).await;

            info!("✅ Finished streaming derivations for {repo_url_outer}");
        }
    });

    StatusCode::ACCEPTED
}
