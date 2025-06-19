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

pub async fn insert_derivation(
    pool: &PgPool,
    commit_hash: &str,
    repo_url: &str,
    target_type: &str,
) -> Result<()> {
    let flake_id: (i32,) = sqlx::query_as("SELECT id FROM tbl_flakes WHERE repo_url = $1")
        .bind(repo_url)
        .fetch_optional(pool)
        .await?
        .context("No flake entry found")?;

    let commit_id: (i32,) =
        sqlx::query_as("SELECT id FROM tbl_commits WHERE flake_id = $1 AND git_commit_hash = $2")
            .bind(flake_id.0)
            .bind(commit_hash)
            .fetch_optional(pool)
            .await?
            .context("No commit found")?;

    sqlx::query(
        r#"INSERT INTO tbl_evaluation_targets (commit_id, type)
           VALUES ($1, $2) ON CONFLICT (commit_id, type) DO NOTHING"#,
    )
    .bind(commit_id.0)
    .bind(target_type)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_derivation_hash(
    pool: &PgPool,
    commit_hash: &str,
    repo_url: &str,
    target_type: &str,
    hash: &str,
) -> Result<()> {
    let flake_id: (i32,) = sqlx::query_as("SELECT id FROM tbl_flakes WHERE repo_url = $1")
        .bind(repo_url)
        .fetch_optional(pool)
        .await?
        .context("No flake entry found")?;

    let commit_id: (i32,) =
        sqlx::query_as("SELECT id FROM tbl_commits WHERE flake_id = $1 AND git_commit_hash = $2")
            .bind(flake_id.0)
            .bind(commit_hash)
            .fetch_optional(pool)
            .await?
            .context("No commit entry found")?;

    let updated = sqlx::query(
        r#"UPDATE tbl_evaluation_targets
           SET hash = $3
           WHERE commit_id = $1 AND type = $2 AND hash IS NULL"#,
    )
    .bind(commit_id.0)
    .bind(target_type)
    .bind(hash)
    .execute(pool)
    .await?
    .rows_affected();

    if updated == 0 {
        warn!("no rows updated");
    }

    Ok(())
}

pub async fn insert_flake(pool: &PgPool, name: &str, repo_url: &str) -> Result<()> {
    sqlx::query("INSERT INTO tbl_flakes (name, repo_url) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(name)
        .bind(repo_url)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn insert_commit(pool: &PgPool, commit_hash: &str, repo_url: &str) -> Result<()> {
    let flake_id: (i32,) = sqlx::query_as("SELECT id FROM tbl_flakes WHERE repo_url = $1")
        .bind(repo_url)
        .fetch_optional(pool)
        .await?
        .context("No flake entry found")?;

    sqlx::query(
        "INSERT INTO tbl_commits (flake_id, git_commit_hash, commit_timestamp)
         VALUES ($1, $2, now()) ON CONFLICT DO NOTHING",
    )
    .bind(flake_id.0)
    .bind(commit_hash)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn insert_host_snapshot(
    pool: &PgPool,
    hostname: &str,
    context: &str,
    system_hash: &str,
    fp: &FingerprintParts,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO tbl_system_states (
            hostname, 
            system_derivation_id,
            context, 
            os, 
            kernel,
            memory_gb, 
            uptime_secs, 
            cpu_brand, 
            cpu_cores,
            board_serial, 
            product_uuid, 
            rootfs_uuid
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
        ON CONFLICT DO NOTHING"#,
    )
    .bind(hostname)
    .bind(system_hash)
    .bind(context)
    .bind(&fp.os)
    .bind(&fp.kernel)
    .bind(fp.memory_gb)
    .bind(fp.uptime_secs as i64)
    .bind(&fp.cpu_brand)
    .bind(fp.cpu_cores as i32)
    .bind(&fp.board_serial)
    .bind(&fp.product_uuid)
    .bind(&fp.rootfs_uuid)
    .execute(pool)
    .await?;

    Ok(())
}
