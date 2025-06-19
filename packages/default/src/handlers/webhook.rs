use axum::{Json, http::StatusCode};
use crystal_forge::flake_watcher::{get_nixos_configurations_at_commit, stream_derivations};
use crystal_forge::webhook_handler::{BoxedHandler, webhook_handler};
use serde_json::Value;
use std::{future::Future, pin::Pin, sync::Arc};
use tokio::sync::Mutex;

pub fn handle_webhook(
    insert_commit_boxed: impl Fn(&str, &str) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>
    + Clone
    + Send
    + Sync
    + 'static,
    insert_derivation_hash_boxed: impl Fn(
        String,
        String,
        String,
        String,
    )
        -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>
    + Clone
    + Send
    + Sync
    + 'static,
    insert_system_name_boxed: impl Fn(
        String,
        String,
        String,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>
    + Clone
    + Send
    + Sync
    + 'static,
) -> impl Fn(Json<Value>) -> Pin<Box<dyn Future<Output = StatusCode> + Send>> + Clone + Send + 'static
{
    move |Json(payload): Json<Value>| {
        let insert_commit_boxed = insert_commit_boxed.clone();
        let insert_derivation_hash_boxed = insert_derivation_hash_boxed.clone();
        let insert_system_name_boxed = insert_system_name_boxed.clone();

        Box::pin(async move {
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

            let insert_system_fn = Arc::new(insert_system_name_boxed.clone());
            let stream_derivations_boxed = {
                let commit = commit_hash.clone();
                move |systems, flake_url: &str, handler: Arc<Mutex<BoxedHandler>>| {
                    let (flake_url, commit, insert_system_fn) = (
                        flake_url.to_string(),
                        commit.clone(),
                        insert_system_fn.clone(),
                    );
                    Box::pin(async move {
                        stream_derivations(systems, &flake_url, &commit, insert_system_fn, handler)
                            .await
                    })
                }
            };

            let get_configs_boxed = {
                let commit_hash = commit_hash.clone();
                Box::new(move |repo_url: String| {
                    let commit = commit_hash.clone();
                    Box::pin(
                        async move { get_nixos_configurations_at_commit(&repo_url, &commit).await },
                    )
                })
            };

            webhook_handler(
                payload,
                insert_commit_boxed,
                get_configs_boxed,
                stream_derivations_boxed,
                insert_derivation_hash_boxed,
            )
            .await
        })
    }
}
