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
    let db_url = db_config.database.to_url();

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
