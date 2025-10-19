//! This module handles incoming webhooks, triggering the fetching of NixOS
//! configurations and streaming derivations to process them. It uses injected
//! async functions for persistence and derivation processing.
use axum::{
    extract::{Json, State},
    http::StatusCode,
};
use serde_json::Value;
use sqlx::PgPool;
use tracing::{error, info, warn};

/// Handles an incoming webhook request for a Git push or merge event.
pub async fn webhook_handler(State(pool): State<PgPool>, Json(payload): Json<Value>) -> StatusCode {
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

    // Extract branch from webhook
    let branch = extract_branch_from_payload(&payload);

    info!("🔗 Repo: {repo_url} @ {commit_hash} (branch: {:?})", branch);

    let pool = pool.clone();
    let time_now = chrono::offset::Utc::now();

    tokio::spawn(async move {
        let normalized_url = normalize_repo_url(&repo_url);

        // Build possible repo URLs to check
        let mut possible_urls = vec![
            repo_url.clone(), // Try exact match first
        ];

        // If we have a branch, try with ?ref=branch
        if let Some(ref branch_name) = branch {
            possible_urls.push(build_repo_url_with_ref(&normalized_url, branch_name));
        }

        // Also try without any ref (for main/master branches)
        possible_urls.push(normalized_url.clone());

        // Try to find matching flake
        match crate::queries::flakes::find_flake_by_repo_urls(&pool, &possible_urls, &repo_url)
            .await
        {
            Ok(Some(flake)) => {
                info!(
                    "✅ Found matching flake: {} (using {})",
                    flake.id, flake.repo_url
                );

                if let Err(e) = crate::queries::commits::insert_commit(
                    &pool,
                    &commit_hash,
                    &flake.repo_url,
                    time_now,
                )
                .await
                {
                    error!("❌ Failed to insert commit: {e}");
                } else {
                    info!("✅ Commit inserted successfully");
                }
            }
            Ok(None) => {
                warn!(
                    "⚠️ No flake found for repo: {} (branch: {:?}). Tried URLs: {:?}",
                    normalized_url, branch, possible_urls
                );

                // Try inserting with the most specific URL we have
                let insert_url = if let Some(branch_name) = branch {
                    build_repo_url_with_ref(&normalized_url, &branch_name)
                } else {
                    repo_url.clone()
                };

                if let Err(e) = crate::queries::commits::insert_commit(
                    &pool,
                    &commit_hash,
                    &insert_url,
                    time_now,
                )
                .await
                {
                    error!("❌ Failed to insert commit: {e}");
                }
            }
            Err(e) => {
                error!("❌ Database error while looking up flake: {e}");
            }
        }
    });

    StatusCode::ACCEPTED
}

fn normalize_repo_url(url: &str) -> String {
    url.split('?').next().unwrap_or(url).to_string()
}

/// Extract branch name from webhook payload
fn extract_branch_from_payload(payload: &Value) -> Option<String> {
    // GitLab sends: "ref": "refs/heads/nixos"
    // GitHub sends: "ref": "refs/heads/main"
    payload
        .pointer("/ref")
        .and_then(|v| v.as_str())
        .and_then(|r| r.strip_prefix("refs/heads/"))
        .map(String::from)
}

/// Build a repo URL with ref parameter
fn build_repo_url_with_ref(base_url: &str, branch: &str) -> String {
    format!("{}?ref={}", base_url, branch)
}
