use crate::db::insert_system_state;
use anyhow::Result;
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use std::ffi::OsStr;
use std::path::PathBuf;

/// Reads a symlink and returns its target as a `PathBuf`.
fn readlink_path(path: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(nix::fcntl::readlink(path)?))
}

/// Handles an inotify event for a specific file name by reading the current system path
/// and invoking a callback to record the system state if the event matches "current-system".
fn handle_event<F, R>(name: &OsStr, readlink_fn: F, insert_fn: R) -> Result<()>
where
    F: Fn(&str) -> Result<PathBuf>,
    R: Fn(&OsStr) -> Result<()>,
{
    if name != OsStr::new("current-system") {
        return Ok(());
    }

    let current_system = readlink_fn("/run/current-system")?;
    println!("Current System: {}", current_system.to_string_lossy());
    insert_fn(current_system.as_os_str())?;

    Ok(())
}

/// Runs a loop that watches for inotify events and handles "current-system" changes using
/// provided readlink and insertion callbacks. Designed for testing and flexibility.
fn watch_system_loop<F, R>(inotify: &mut Inotify, readlink_fn: F, insert_fn: R) -> Result<()>
where
    F: Fn(&str) -> Result<PathBuf>,
    R: Fn(&OsStr) -> Result<()>,
{
    println!("Watching /run for changes to current-system...");
    let init = OsStr::new("current-system");
    handle_event(init, &readlink_fn, &insert_fn)?;
    loop {
        for event in inotify.read_events()? {
            if let Some(name) = event.name {
                println!("Detected change to /run/current-system");
                handle_event(&name, &readlink_fn, &insert_fn)?;
            }
        }
    }
}

pub fn watch_system() -> Result<()> {
    let mut inotify = Inotify::init(InitFlags::empty())?;
    inotify.add_watch(
        "/run",
        AddWatchFlags::IN_CREATE | AddWatchFlags::IN_MOVED_TO,
    )?;

    watch_system_loop(&mut inotify, readlink_path, insert_system_state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn test_handle_event_triggers_on_current_system() {
        let called = RefCell::new(false);

        let readlink_mock = |_path: &str| Ok(PathBuf::from("/nix/store/fake-system"));
        let insert_mock = |os: &OsStr| {
            assert_eq!(os.to_string_lossy(), "/nix/store/fake-system");
            *called.borrow_mut() = true;
            Ok(())
        };

        let result = handle_event(OsStr::new("current-system"), readlink_mock, insert_mock);
        assert!(result.is_ok());
        assert!(*called.borrow());
    }

    #[test]
    fn test_handle_event_ignores_other_files() {
        let readlink_mock = |_path: &str| panic!("should not be called");
        let insert_mock = |_os: &OsStr| panic!("should not be called");

        let result = handle_event(OsStr::new("other-file"), readlink_mock, insert_mock);
        assert!(result.is_ok());
    }
}
