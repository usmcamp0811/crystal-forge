use crate::config;
use crate::sys_fingerprint::FingerprintParts;
use crate::system_watcher::SystemPayload;
use anyhow::{Context, Result};
use serde_json;
use std::ffi::OsStr;
use std::path::Path;
use std::{env, fs};
use tokio_postgres::{Client, NoTls};
use tracing::{debug, error, info, trace, warn};

pub async fn get_db_client() -> Result<Client> {
    let db_config = config::load_config()?;
    let db_url = db_config
        .database
        .as_ref()
        .context("missing [database] section in config")?
        .to_url();

    let (client, connection) = tokio_postgres::connect(&db_url, NoTls).await?;
    tokio::spawn(connection);
    Ok(client)
}

// A function that gets all commits that do not have systems attached to them.
// situation: maybe you got a webhook trigger and there was some problem accessing the flake
// this could be a temporary thing like internet went out or something else shit the bed
// so lets query for it and see if we can do something at startup
pub async fn get_commits_without_systems() -> Result<Vec<(String, String, String)>> {
    let client = get_db_client().await?;

    let rows = client
        .query(
            "
            SELECT c.git_commit_hash, f.repo_url, f.name
            FROM tbl_commits c
            LEFT JOIN tbl_system_builds b ON c.id = b.commit_id
            LEFT JOIN tbl_flakes f ON c.flake_id = f.id
            WHERE b.commit_id IS NULL
            ",
            &[],
        )
        .await?;

    let results = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<_, String>(0), // c.git_commit_hash
                row.get::<_, String>(1), // f.repo_url
                row.get::<_, String>(2), // f.name
            )
        })
        .collect();

    Ok(results)
}

// A function that gets all the systems that we don't have derivation hashes for.
// maybe because we crashed or restarted before they were eval'd or some other reason.
pub async fn get_systems_to_eval() -> Result<Vec<(String, String, String, String)>> {
    let client = get_db_client().await?;

    let rows = client
        .query(
            "
            SELECT f.name, f.repo_url, c.git_commit_hash, b.system_name
            FROM tbl_system_builds b
            INNER JOIN tbl_commits c ON b.commit_id = c.id
            INNER JOIN tbl_flakes f ON c.flake_id = f.id
            WHERE b.derivation_hash IS NULL
            ",
            &[],
        )
        .await?;

    let results = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<_, String>(0), // f.name
                row.get::<_, String>(1), // f.repo_url
                row.get::<_, String>(2), // c.git_commit_hash
                row.get::<_, String>(3), // b.system_name
            )
        })
        .collect();

    Ok(results)
}

pub async fn insert_system_name(
    commit_hash: &str,
    repo_url: &str,
    system_name: &str,
) -> Result<()> {
    debug!(
        "Inserting System {} - from {} @ {}",
        system_name, repo_url, commit_hash
    );
    let client = get_db_client().await?;

    // Get flake_id
    let flake_row = client
        .query_opt(
            "SELECT id FROM tbl_flakes WHERE repo_url = $1",
            &[&repo_url],
        )
        .await?
        .with_context(|| format!("No flake entry found for given repo_url: {}", repo_url))?;

    let flake_id: i32 = flake_row.get("id");

    // Get commit_id
    let commit_row = client
        .query_opt(
            "SELECT id FROM tbl_commits WHERE flake_id = $1 AND git_commit_hash = $2",
            &[&flake_id, &commit_hash],
        )
        .await?
        .context("No commit entry found for given commit_hash and flake_id")?;

    let commit_id: i32 = commit_row.get("id");

    // Insert system_name (no hash yet)
    client
        .execute(
            "INSERT INTO tbl_system_builds (commit_id, system_name)
             VALUES ($1, $2)
             ON CONFLICT (commit_id, system_name) DO NOTHING",
            &[&commit_id, &system_name],
        )
        .await?;

    info!("📥 inserted system: {system_name} for commit {commit_hash}");
    Ok(())
}

pub async fn insert_derivation_hash(
    commit_hash: &str,
    repo_url: &str,
    system_name: &str,
    derivation_hash: &str,
) -> Result<()> {
    debug!(
        "🔍 insert_derivation_hash called with: system={}, repo_url={}, commit_hash={}, derivation_hash={}",
        system_name, repo_url, commit_hash, derivation_hash
    );

    let client = get_db_client().await?;

    debug!("🔗 querying flake_id from repo_url: {}", repo_url);
    let flake_row = client
        .query_opt(
            "SELECT id FROM tbl_flakes WHERE repo_url = $1",
            &[&repo_url],
        )
        .await?
        .context("No flake entry found for given repo_url")?;
    let flake_id: i32 = flake_row.get("id");

    debug!(
        "🔗 querying commit_id for flake_id={} and commit_hash={}",
        flake_id, commit_hash
    );
    let commit_row = client
        .query_opt(
            "SELECT id FROM tbl_commits WHERE flake_id = $1 AND git_commit_hash = $2",
            &[&flake_id, &commit_hash],
        )
        .await?
        .context("No commit entry found for given commit_hash and flake_id")?;
    let commit_id: i32 = commit_row.get("id");

    debug!(
        "📝 updating derivation_hash for system={} with commit_id={} and hash={}",
        system_name, commit_id, derivation_hash
    );

    let updated = client
        .execute(
            "UPDATE tbl_system_builds
             SET derivation_hash = $3
             WHERE commit_id = $1 AND system_name = $2 AND derivation_hash IS NULL",
            &[&commit_id, &system_name, &derivation_hash],
        )
        .await?;

    if updated == 0 {
        warn!(
            "⚠️ no rows updated for system={} commit_id={}",
            system_name, commit_id
        );
    } else {
        info!("✅ updated derivation: {system_name} => {derivation_hash}");
    }

    Ok(())
}

pub async fn insert_flake(name: &str, repo_url: &str) -> Result<()> {
    let client = get_db_client().await?;

    client
        .execute(
            "INSERT INTO tbl_flakes (name, repo_url)
             VALUES ($1, $2)
             ON CONFLICT DO NOTHING",
            &[&name, &repo_url],
        )
        .await?;

    info!("✅ inserted flake: {name} ({repo_url})");
    Ok(())
}

pub async fn insert_commit(commit_hash: &str, repo_url: &str) -> Result<()> {
    let client = get_db_client().await?;

    // look up flake_id from repo_url
    let flake_row = client
        .query_opt(
            "SELECT id FROM tbl_flakes WHERE repo_url = $1",
            &[&repo_url],
        )
        .await?
        .context("No flake entry found for given repo_url")?;

    let flake_id: i32 = flake_row.get("id");

    // insert commit
    client
        .execute(
            "INSERT INTO tbl_commits (flake_id, git_commit_hash, commit_timestamp)
             VALUES ($1, $2, now())
             ON CONFLICT DO NOTHING",
            &[&flake_id, &commit_hash],
        )
        .await?;

    info!("✅ inserted commit: {commit_hash} (flake_id={flake_id})");
    Ok(())
}

pub async fn insert_system_state(
    hostname: &str,
    context: &str,
    system_hash: &str,
    fingerprint: &FingerprintParts,
) -> Result<()> {
    let client = get_db_client().await?;

    client
        .execute(
            "INSERT INTO tbl_system_states (
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
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12
        ) ON CONFLICT DO NOTHING",
            &[
                &hostname,
                &system_hash,
                &context,
                &fingerprint.os,
                &fingerprint.kernel,
                &fingerprint.memory_gb,
                &(fingerprint.uptime_secs as i64), // u64 → i64
                &fingerprint.cpu_brand,
                &(fingerprint.cpu_cores as i32), // usize → i32
                &fingerprint.board_serial,
                &fingerprint.product_uuid,
                &fingerprint.rootfs_uuid,
            ],
        )
        .await?;

    info!("✅ inserted system state for {hostname} with hash {system_hash}");
    Ok(())
}

pub async fn init_db() -> Result<()> {
    let client = get_db_client().await?;
    info!("======== INITIALIZING DATABASE ========");

    client
        .batch_execute(
            "
            CREATE TABLE IF NOT EXISTS tbl_flakes (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                repo_url TEXT NOT NULL UNIQUE
            );

            CREATE TABLE IF NOT EXISTS tbl_commits (
                id SERIAL PRIMARY KEY,
                flake_id INT NOT NULL REFERENCES tbl_flakes(id) ON DELETE CASCADE,
                git_commit_hash TEXT NOT NULL,
                commit_timestamp TIMESTAMPTZ NOT NULL,
                UNIQUE(flake_id, git_commit_hash)
            );

            CREATE TABLE IF NOT EXISTS tbl_system_builds (
                id SERIAL PRIMARY KEY,
                commit_id INT NOT NULL REFERENCES tbl_commits(id) ON DELETE CASCADE,
                system_name TEXT NOT NULL,
                derivation_hash TEXT,
                build_timestamp TIMESTAMPTZ DEFAULT now(),
                UNIQUE(commit_id, system_name)
            );

            CREATE TABLE IF NOT EXISTS tbl_system_states (
                id SERIAL PRIMARY KEY,
                hostname TEXT NOT NULL,
                system_derivation_id TEXT NOT NULL,
                context TEXT NOT NULL,
                os TEXT,
                kernel TEXT,
                memory_gb DOUBLE PRECISION,
                uptime_secs BIGINT,
                cpu_brand TEXT,
                cpu_cores INT,
                board_serial TEXT,
                product_uuid TEXT,
                rootfs_uuid TEXT
            );
            ",
        )
        .await?;

    Ok(())
}
