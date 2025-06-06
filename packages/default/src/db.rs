use crate::config;
use anyhow::{Context, Result};
use postgres::{Client, NoTls};
use std::ffi::OsStr;
use std::path::Path;
use std::{env, fs};

pub fn insert_system_state(current_system: &OsStr) -> Result<()> {
    let db_config = config::load_config()?;
    let db_url = db_config.to_url();
    let mut client = Client::connect(&db_url, NoTls)?;

    let hostname = hostname::get()?.to_string_lossy().into_owned();
    let system_hash = Path::new(current_system)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| current_system.to_string_lossy().into_owned());

    client.execute(
        "INSERT INTO system_state (hostname, system_derivation_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        &[&hostname, &system_hash],
    )?;

    Ok(())
}

// pub fn post_system_state(current_system: &OnStr) -> Result<()> {
//     let server_config = config::load_config()?;
// }
