//! This module handles incoming webhooks, triggering the fetching of NixOS
//! configurations and streaming derivations to process them. It uses injected
//! async functions for persistence and derivation processing.

use anyhow::{Context, Result};
use axum::{Json, http::StatusCode};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// Handles an incoming webhook request for a Git push or merge event.
pub async fn webhook_handler(
    payload: serde_json::Value,
    pool: &sqlx::PgPool,
) -> axum::http::StatusCode {
    use axum::http::StatusCode;
    use tracing::{error, info, warn};

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

    let pool = pool.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::queries::commits::insert_commit(&pool, &commit_hash, &repo_url).await
        {
            error!("❌ Failed to insert commit: {e}");
        } else {
            info!("✅ Commit inserted successfully");
        }
    });

    StatusCode::ACCEPTED
}
