use crystal_forge::config;
use crystal_forge::db;
use crystal_forge::system_watcher;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::load_config()?;
    // TODO: Update watch_system to take in the private key path
    system_watcher::watch_system()
}
