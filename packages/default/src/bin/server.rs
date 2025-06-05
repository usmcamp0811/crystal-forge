use server::config;
use server::db;
use server::system_watcher;

use anyhow::Result;

fn main() -> Result<()> {
    let config = config::load_config()?;
    let db_url = config.to_url();

    config::validate_db_connection(&db_url)?;
}
