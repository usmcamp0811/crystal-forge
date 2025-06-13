use crate::db::{insert_commit, insert_derivation_hash};
use crate::flake_watcher::{get_nixos_configurations, stream_derivations};
use axum::{Json, http::StatusCode};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

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
    println!("========== PROCESSING WEBHOOK ==========");
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
        eprintln!("❌ insert_commit_fn failed: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    tokio::spawn({
        let repo_url_outer = repo_url.clone();
        let commit_hash_outer = commit_hash.clone();
        let get_configs_fn = get_configs_fn.clone();
        let stream_fn = stream_fn.clone();
        let insert_deriv_hash_fn = insert_deriv_hash_fn.clone();

        async move {
            println!("🔧 starting config fetch for {repo_url_outer}");

            match get_configs_fn(repo_url_outer.clone()).await {
                Ok(configs) => {
                    println!("📦 fetched configs: {:?}", configs);

                    let repo_url_for_closure = repo_url_outer.clone();
                    let handle_result: Arc<Mutex<BoxedHandler>> =
                        Arc::new(Mutex::new(Box::new(move |system: String, hash: String| {
                            let repo_url = repo_url_for_closure.clone();
                            let commit_hash = commit_hash_outer.clone();
                            println!("📝 handling system: {system} => {hash}");
                            Box::pin(async move {
                                let res =
                                    insert_deriv_hash_fn(commit_hash, repo_url, system, hash).await;
                                if let Err(e) = &res {
                                    eprintln!("❌ insert_derivation_hash error: {e}");
                                }
                                res
                            })
                        })));

                    println!("🚀 starting stream_derivations");
                    if let Err(e) = stream_fn(configs, &repo_url_outer, handle_result).await {
                        eprintln!("❌ stream_derivations error: {e}");
                    }
                }
                Err(e) => {
                    eprintln!("❌ get_configs_fn error: {e:?}");
                }
            }
        }
    });

    StatusCode::ACCEPTED
}
