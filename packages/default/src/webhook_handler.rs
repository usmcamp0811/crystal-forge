use crate::db::{insert_commit, insert_derivation_hash};
use crate::flake_watcher::{get_nixos_configurations, stream_derivations};
use anyhow::Result;
use axum::{Json, http::StatusCode};
use crystal_forge::db::{insert_commit, insert_system_state};
use serde_json::Value;
use serde_json::Value;

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

    // Immediately record the commit
    if let Err(e) = insert_commit(&commit_hash, &repo_url).await {
        eprintln!("❌ failed to insert commit: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    // Spawn the background task
    tokio::spawn(async move {
        if let Ok(configs) = get_nixos_configurations(repo_url.clone()) {
            let _ = stream_derivations(configs, &repo_url, |system, hash| async {
                insert_derivation_hash(&commit_hash, &repo_url, &system, &hash).await
            })
            .await;
        }
    });

    StatusCode::ACCEPTED
}

// Make a Queue that gets populated by webhook triggers
// this will keep us from being bogged down by trying to eval too many systems at once
// we could also in the future distribute this queue to multiple workers

// queue = []
// queue.append(webhook trigger)
// all_systems = get_nixos_configurations(repo_url: String)
// stream_derivations(all_systems) -> Postgres function to save derivation hash to crystal_forge.system_build table
