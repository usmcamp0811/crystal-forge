use anyhow::Result;
use nix::fcntl::readlink;
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use postgres::{Client, NoTls};
use std::path::Path;
use std::{ffi::OsStr, os::unix::ffi::OsStrExt, path::PathBuf};

fn log_to_db(current_system: &OsStr) -> Result<()> {
    let mut client = Client::connect(
        "host=reckless user=crystal_forge password=password dbname=crystal_forge",
        NoTls,
    )?;

    let hostname = hostname::get()?.to_string_lossy().into_owned();
    let system_hash = Path::new(current_system)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| current_system.to_string_lossy().into_owned());

    client.execute(
        "INSERT INTO system_state (hostname, system_hash) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        &[&hostname, &system_hash],
    )?;

    Ok(())
}

fn main() -> Result<()> {
    let target = OsStr::new("current-system");

    let mut inotify = Inotify::init(InitFlags::empty())?;
    inotify.add_watch(
        "/run",
        AddWatchFlags::IN_CREATE | AddWatchFlags::IN_MOVED_TO,
    )?;

    println!("Watching /run for changes to current-system...");

    loop {
        for event in inotify.read_events()? {
            let Some(name) = event.name else { continue };
            if name != target {
                continue;
            }

            let current_system = match readlink("/run/current-system") {
                Ok(path) => path,
                Err(_) => continue,
            };

            println!("Detected change to /run/current-system");
            println!("Current System: {}", current_system.to_string_lossy());
            log_to_db(&current_system)?;
        }
    }
}
