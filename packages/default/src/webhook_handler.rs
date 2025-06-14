/// This module handles incoming webhooks, triggering the fetching of NixOS
/// configurations and streaming derivations to process them.
///
/// It expects a webhook payload containing repository and commit information,
/// and uses injected async functions for database and derivation operations.
use crate::db::{insert_commit, insert_derivation_hash};
use crate::flake_watcher::{get_nixos_configurations, stream_derivations};
use axum::{Json, http::StatusCode};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

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
    println!("========== PROCESSING WEBHOOK ==========");

    // Extract repo URL from payload
    let Some(repo_url) = payload
        .pointer("/repository/clone_url")
        .or_else(|| payload.pointer("/project/web_url"))
        .and_then(|v| v.as_str())
        .map(String::from)
    else {
        return StatusCode::BAD_REQUEST;
    };

    // Extract commit hash from payload
    let Some(commit_hash) = payload
        .pointer("/after")
        .or_else(|| payload.pointer("/checkout_sha"))
        .and_then(|v| v.as_str())
        .map(String::from)
    else {
        return StatusCode::BAD_REQUEST;
    };

    // Try to insert commit into DB
    if let Err(e) = insert_commit_fn(&commit_hash, &repo_url).await {
        eprintln!("❌ insert_commit_fn failed: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    // Spawn async task to fetch and process configurations
    tokio::spawn({
        let repo_url_outer = repo_url.clone();
        let commit_hash_outer = commit_hash.clone();

        let get_configs_fn = get_configs_fn.clone();
        let stream_fn = stream_fn.clone();
        let insert_deriv_hash_fn = insert_deriv_hash_fn.clone();

        async move {
            println!("🔧 starting config fetch for {repo_url_outer}");
            if let Ok(configs) = get_configs_fn(repo_url_outer.clone()).await {
                println!("📦 fetched configs: {:?}", configs);

                if configs.is_empty() {
                    println!("⚠️ no configs found, exiting stream task");
                    return;
                }

                let repo_url_for_closure = repo_url_outer.clone();
                let handle_result: Arc<Mutex<BoxedHandler>> =
                    Arc::new(Mutex::new(Box::new(move |system: String, hash: String| {
                        let repo_url = repo_url_for_closure.clone();
                        let commit_hash = commit_hash_outer.clone();
                        println!("📝 handling system: {system} => {hash}");
                        Box::pin({
                            let insert_deriv_hash_fn = insert_deriv_hash_fn.clone();
                            async move {
                                if let Err(e) =
                                    insert_deriv_hash_fn(commit_hash, repo_url, system, hash).await
                                {
                                    eprintln!("❌ insert_deriv_hash_fn failed: {e:?}");
                                }
                                Ok(())
                            }
                        })
                    })));

                stream_fn(configs, &repo_url_outer, handle_result).await;
                println!("✅ finished streaming derivations");
            }
        }
    });

    StatusCode::ACCEPTED
}
