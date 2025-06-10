use crate::config;
use crate::sys_fingerprint::FingerprintParts;
use crate::system_watcher::SystemPayload;
use anyhow::{Context, Result};
use serde_json;
use std::ffi::OsStr;
use std::path::Path;
use std::{env, fs};
use tokio_postgres::{Client, NoTls};

pub async fn insert_system_state(
    hostname: &str,
    context: &str,
    system_hash: &str,
    fingerprint: &FingerprintParts,
) -> Result<()> {
    let db_config = config::load_config()?;
    let db_url = db_config
        .database
        .as_ref()
        .expect("missing [database] section in config")
        .to_url();

    let (client, connection) = tokio_postgres::connect(&db_url, NoTls).await?;
    tokio::spawn(connection); // drive the connection

    client
        .execute(
            "INSERT INTO system_state (
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
    let db_config = config::load_config()?;
    let db_url = db_config
        .database
        .as_ref()
        .expect("missing [database] section in config")
        .to_url();

    let (client, connection) = tokio_postgres::connect(&db_url, NoTls).await?;
    tokio::spawn(connection);

    client
        .batch_execute(
            "
            CREATE TABLE IF NOT EXISTS flake (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                repo_url TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS commit (
                id SERIAL PRIMARY KEY,
                flake_id INT NOT NULL REFERENCES flake(id) ON DELETE CASCADE,
                git_commit_hash TEXT NOT NULL,
                commit_timestamp TIMESTAMPTZ NOT NULL,
                UNIQUE(flake_id, git_commit_hash)
            );

            CREATE TABLE IF NOT EXISTS system_build (
                id SERIAL PRIMARY KEY,
                commit_id INT NOT NULL REFERENCES commit(id) ON DELETE CASCADE,
                system_name TEXT NOT NULL,
                derivation_hash TEXT NOT NULL,
                build_timestamp TIMESTAMPTZ DEFAULT now(),
                UNIQUE(commit_id, system_name)
            );

            CREATE TABLE IF NOT EXISTS system_state (
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
