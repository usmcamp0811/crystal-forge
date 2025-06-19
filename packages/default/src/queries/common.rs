use crate::db::get_db_pool;
use crate::sys_fingerprint::FingerprintParts;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{Row, postgres::PgPool};
use tracing::{debug, info, warn};

pub async fn get_commits_without_targets(pool: &PgPool) -> Result<Vec<(String, String, String)>> {
    let rows = sqlx::query!(
        r#"
        SELECT c.git_commit_hash, f.repo_url, f.name
        FROM tbl_commits c
        LEFT JOIN tbl_evaluation_targets t ON c.id = t.commit_id
        INNER JOIN tbl_flakes f ON c.flake_id = f.id
        WHERE t.commit_id IS NULL
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| (r.git_commit_hash, r.repo_url, r.name))
        .collect())
}

pub async fn get_pending_derivations(
    pool: &PgPool,
) -> Result<Vec<(String, String, String, String)>> {
    let rows = sqlx::query!(
        r#"
        SELECT f.name, f.repo_url, c.git_commit_hash, d.type
        FROM tbl_evaluation_targets d
        JOIN tbl_commits c ON d.commit_id = c.id
        JOIN tbl_flakes f ON c.flake_id = f.id
        WHERE d.hash IS NULL
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| (r.name, r.repo_url, r.git_commit_hash, r.r#type))
        .collect())
}








