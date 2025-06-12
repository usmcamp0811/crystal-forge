use crate::config;
use crate::sys_fingerprint::FingerprintParts;
use crate::system_watcher::SystemPayload;
use anyhow::{Context, Result};
use serde_json;
use std::ffi::OsStr;
use std::path::Path;
use std::{env, fs};
use tokio_postgres::{Client, NoTls};

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

    Ok(())
}

pub async fn init_db() -> Result<()> {
    let client = get_db_client().await?;

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
                flake_id INT NOT NULL REFERENCES flake(id) ON DELETE CASCADE,
                git_commit_hash TEXT NOT NULL,
                commit_timestamp TIMESTAMPTZ NOT NULL,
                UNIQUE(flake_id, git_commit_hash)
            );

            CREATE TABLE IF NOT EXISTS tbl_system_builds (
                id SERIAL PRIMARY KEY,
                commit_id INT NOT NULL REFERENCES commit(id) ON DELETE CASCADE,
                system_name TEXT NOT NULL,
                derivation_hash TEXT NOT NULL,
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
