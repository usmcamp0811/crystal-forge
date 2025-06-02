mod config;
mod db;
mod system_watcher;
use anyhow::Result;

fn main() -> Result<()> {
    system_watcher::watch_system()
}
