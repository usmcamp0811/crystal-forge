use anyhow::Result;

mod db;
mod system_watcher;

fn main() -> Result<()> {
    system_watcher::watch_system()
}
