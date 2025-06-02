use crate::db::insert_system_state;
use anyhow::Result;
use nix::fcntl::readlink;
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use std::ffi::OsStr;

pub fn watch_system() -> Result<()> {
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
            insert_system_state(&current_system)?;
        }
    }
}
