use crate::db::{insert_commit, insert_derivation_hash};
use crate::flake_watcher::{get_nixos_configurations, stream_derivations};
use axum::{Json, http::StatusCode};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

type BoxedHandler = Box<
    dyn Fn(String, String) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
        + Send
        + Sync,
>;

pub async fn webhook_handler(Json(payload): Json<Value>) -> StatusCode {
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

    if let Err(e) = insert_commit(&commit_hash, &repo_url).await {
        eprintln!("❌ failed to insert commit: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    tokio::spawn({
        let repo_url_outer = repo_url.clone();
        let commit_hash_outer = commit_hash.clone();

        async move {
            if let Ok(configs) = get_nixos_configurations(repo_url_outer.clone()).await {
                let repo_url_for_closure = repo_url_outer.clone();
                let commit_hash_for_closure = commit_hash_outer.clone();

                let handle_result: Arc<Mutex<BoxedHandler>> =
                    Arc::new(Mutex::new(Box::new(move |system: String, hash: String| {
                        let repo_url = repo_url_for_closure.clone();
                        let commit_hash = commit_hash_for_closure.clone();
                        Box::pin(async move {
                            insert_derivation_hash(&commit_hash, &repo_url, &system, &hash).await
                        })
                    })));

                let _ = stream_derivations(configs, &repo_url_outer, handle_result).await;
            }
        }
    });

    StatusCode::ACCEPTED
}
