use crate::db::{insert_commit, insert_derivation_hash};
use crate::flake_watcher::{get_all_derivations, get_nixos_configurations, stream_derivations};
use axum::{Json, http::StatusCode};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

pub type BoxedHandler = Box<
    dyn Fn(String, String) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
        + Send
        + Sync,
>;

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

    if let Err(e) = insert_commit_fn(&commit_hash, &repo_url).await {
        error!("❌ Failed to insert commit: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    let get_configs = get_configs_fn.clone();
    let Ok(systems) = get_configs(repo_url.clone()).await else {
        error!("❌ Failed to get nixos configurations for: {repo_url}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    };

    let stream = stream_fn.clone();
    let insert = insert_deriv_hash_fn.clone();
    let repo_for_spawn = repo_url.clone();
    let commit_for_spawn = commit_hash.clone();
    let systems_for_spawn = systems.clone();

    tokio::spawn(async move {
        run_config_streaming_task(
            systems_for_spawn,
            repo_for_spawn,
            commit_for_spawn,
            stream,
            insert,
        )
        .await;
    });

    tokio::spawn(async move {
        match get_all_derivations(systems, &repo_url, &commit_hash).await {
            Ok(results) => {
                for (system, hash) in results {
                    debug!("✅ {system} => {hash}");
                }
            }
            Err(e) => error!("❌ get_all_derivations failed: {e:?}"),
        }
    });

    StatusCode::ACCEPTED
}

async fn run_config_streaming_task<F3, F4>(
    systems: Vec<String>,
    repo_url: String,
    commit_hash: String,
    stream_fn: F3,
    insert_deriv_hash_fn: F4,
) where
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
    if systems.is_empty() {
        warn!("⚠️ No NixOS configurations found in flake");
        return;
    }

    let handler: Arc<Mutex<BoxedHandler>> = {
        let repo_url = repo_url.clone();
        let commit_hash = commit_hash.clone();
        let insert_deriv_hash_fn = insert_deriv_hash_fn.clone();

        Arc::new(Mutex::new(Box::new(move |system: String, hash: String| {
            let repo_url = repo_url.clone();
            let commit_hash = commit_hash.clone();
            let insert_deriv_hash_fn = insert_deriv_hash_fn.clone();

            Box::pin(async move {
                if let Err(e) = insert_deriv_hash_fn(commit_hash, repo_url, system, hash).await {
                    error!("❌ Failed to insert derivation hash: {e:?}");
                }
                Ok(())
            })
        })))
    };

    stream_fn(systems, &repo_url, handler).await;
    debug!("✅ Finished streaming derivations");
}
