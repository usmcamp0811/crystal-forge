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
    system_hash: &str,
    fingerprint: &FingerprintParts,
) -> Result<()> {
    let db_config = config::load_config()?;
    let db_url = db_config.database.to_url();

    let (client, connection) = tokio_postgres::connect(&db_url, NoTls).await?;
    tokio::spawn(connection); // drive the connection

    let fingerprint_json = serde_json::to_string(fingerprint)?;

    client.execute(
        "INSERT INTO system_state (hostname, system_derivation_id, fingerprint) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        &[&hostname, &system_hash, &fingerprint_json],
    ).await?;

    Ok(())
}
