use crate::config;
use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::path::Path;
use std::{env, fs};
use tokio_postgres::{Client, NoTls};

pub async fn insert_system_state(
    hostname: &str,
    system_hash: &OsStr,
    fingerprint: &str,
) -> Result<()> {
    let db_config = config::load_config()?;
    let db_url = db_config.database.to_url();

    let (client, connection) = tokio_postgres::connect(&db_url, NoTls).await?;
    tokio::spawn(connection); // drive the connection

    let system_hash = system_hash.to_string_lossy();

    client.execute(
        "INSERT INTO system_state (hostname, system_derivation_id, fingerprint) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        &[&hostname, &system_hash, &fingerprint],
    ).await?;

    Ok(())
}
