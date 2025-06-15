use crate::db::{insert_commit, insert_derivation_hash};
use crate::flake_watcher::{get_nixos_configurations, stream_derivations};
use axum::{Json, http::StatusCode};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, trace, warn};

/// This module handles incoming webhooks, triggering the fetching of NixOS
/// configurations and streaming derivations to process them.
///
/// It expects a webhook payload containing repository and commit information,
/// and uses injected async functions for database and derivation operations.

/// Type alias for a boxed async handler function that inserts a derivation hash.
/// The function takes two strings: the system name and the derivation hash.
pub type BoxedHandler = Box<
    dyn Fn(String, String) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
        + Send
        + Sync,
>;

/// Handles webhook events for git pushes and triggers downstream processing.
///
/// # Arguments
/// - `payload`: The incoming webhook JSON payload.
/// - `insert_commit_fn`: Async function to insert commit hash into DB.
/// - `get_configs_fn`: Async function to get NixOS configurations from a repo URL.
/// - `stream_fn`: Async function to stream derivations and handle each result.
/// - `insert_deriv_hash_fn`: Async function to insert derivation hash into DB.
///
/// # Returns
/// `StatusCode::ACCEPTED` on success, appropriate error code on failure.
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
    info!("========== PROCESSING WEBHOOK ==========");

    // Extract required fields
    let Some(repo_url) = payload
        .pointer("/repository/clone_url")
        .or_else(|| payload.pointer("/project/web_url"))
        .and_then(|v| v.as_str())
        .map(String::from)
    else {
        return StatusCode::BAD_REQUEST;
    };

    let Some(commit_hash) = payload
        .pointer("/after")
        .or_else(|| payload.pointer("/checkout_sha"))
        .and_then(|v| v.as_str())
        .map(String::from)
    else {
        return StatusCode::BAD_REQUEST;
    };

    // Insert the commit
    if let Err(e) = insert_commit_fn(&commit_hash, &repo_url).await {
        error!("❌ Failed to insert commit: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    // Fire-and-forget task
    tokio::spawn(run_config_streaming_task(
        repo_url,
        commit_hash,
        get_configs_fn,
        stream_fn,
        insert_deriv_hash_fn,
    ));

    StatusCode::ACCEPTED
}

async fn run_config_streaming_task<F2, F3, F4>(
    repo_url: String,
    commit_hash: String,
    get_configs_fn: F2,
    stream_fn: F3,
    insert_deriv_hash_fn: F4,
) where
    F2: Fn(String) -> Pin<Box<dyn Future<Output = Result<Vec<String>, anyhow::Error>> + Send>>
        + Clone
        + Send
        + 'static,
    F3: Fn(Vec<String>, &str, Arc<Mutex<BoxedHandler>>) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Clone
        + Send
        + 'static,
    F4: Fn(
            String,
            String,
            String,
            String,
        ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
        + Clone
        + Send
        + Sync
        + 'static,
{
    info!("🔧 Fetching configurations for {repo_url}");

    let Ok(configs) = get_configs_fn(repo_url.clone()).await else {
        error!("❌ Failed to fetch configs for {repo_url}");
        return;
    };

    if configs.is_empty() {
        warn!("⚠️ No NixOS configurations found in flake");
        return;
    }

    let handler: Arc<Mutex<BoxedHandler>> = Arc::new(Mutex::new(Box::new({
        let repo_url = repo_url.clone();
        let commit_hash = commit_hash.clone();
        let insert_deriv_hash_fn = insert_deriv_hash_fn.clone();

        move |system, hash| {
            let repo_url = repo_url.clone();
            let commit_hash = commit_hash.clone();
            let insert_deriv_hash_fn = insert_deriv_hash_fn.clone();

            Box::pin(async move {
                if let Err(e) = insert_deriv_hash_fn(commit_hash, repo_url, system, hash).await {
                    error!("❌ Failed to insert derivation hash: {e:?}");
                }
                Ok(())
            })
        }
    })));

    stream_fn(configs, &repo_url, handler).await;
    debug!("✅ Finished streaming derivations");
}
