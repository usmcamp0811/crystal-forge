use crystal_forge::db;
use crystal_forge::config;
use crystal_forge::system_watcher;

use anyhow::Result;

fn main() -> Result<()> {
    let config = config::load_config()?;
    let db_url = config.to_url();

    config::validate_db_connection(&db_url)?;
    Ok(())
}
