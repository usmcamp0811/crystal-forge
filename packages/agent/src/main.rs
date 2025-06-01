use anyhow::Result;
use nix::fcntl::readlink;
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use nix::sys::sysinfo;
use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

fn main() -> Result<()> {
    let target = OsStr::new("current-system");

    let mut inotify = Inotify::init(InitFlags::empty())?;
    inotify.add_watch(
        "/run",
        AddWatchFlags::IN_CREATE | AddWatchFlags::IN_MOVED_TO,
    )?;

    println!("Watching /run for changes to current-system...");

    loop {
        let events = inotify.read_events()?;
        for event in events {
            if let Some(name) = event.name {
                if name == target {
                    if let Ok(current_system) = readlink("/run/current-system") {
                        println!("Detected change to /run/current-system");
                        println!("Current System: {}", current_system.to_string_lossy());
                    }
                }
            }
        }
    }
}
