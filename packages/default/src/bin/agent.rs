use agent::config;
use agent::db;
use agent::system_watcher;

use anyhow::Result;

fn main() -> Result<()> {
    let config = config::load_config()?;
    let db_url = config.to_url();

    config::validate_db_connection(&db_url)?;
    system_watcher::watch_system()
}
