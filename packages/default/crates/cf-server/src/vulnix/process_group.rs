//! Owns process groups used by server-local CVE scanner commands.

use anyhow::{Context, Result, bail};
use tokio::process::{Child, Command};
use tracing::warn;

/// Configures a command as the leader of a new Unix process group.
pub(crate) fn isolate(command: &mut Command) {
    command.kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
}

/// Kills a scanner command's complete process group on cancellation or drop.
///
/// CONCURRENCY: `Drop` sends `SIGKILL` synchronously. A dropped worker future
/// therefore cannot release its execution lock while a `nix` or `vulnix`
/// descendant continues to run. Explicit termination also waits for the direct
/// child. Unix descendants are reparented and reaped by the system subreaper;
/// Crystal Forge can wait only for the direct child that it spawned.
pub(crate) struct ScannerProcessGroup {
    child: Option<Child>,
    pgid: i32,
    child_reaped: bool,
    armed: bool,
}

impl ScannerProcessGroup {
    /// Creates a guard for a child spawned after [`isolate`] was called.
    pub(crate) fn new(child: Child, process_name: &str) -> Result<Self> {
        let pgid = child
            .id()
            .context(format!("spawned {process_name} without a PID"))?
            .try_into()
            .context(format!("spawned {process_name} PID does not fit in pid_t"))?;
        if pgid <= 0 {
            bail!("refusing to guard invalid scanner process group ID {pgid}");
        }
        Ok(Self {
            child: Some(child),
            pgid,
            child_reaped: false,
            armed: true,
        })
    }

    /// Returns the guarded direct child while the guard is armed.
    pub(crate) fn child_mut(&mut self) -> &mut Child {
        self.child
            .as_mut()
            .expect("scanner process child is present while guard is armed")
    }

    /// Waits for and reaps the direct child while retaining descendant cleanup.
    pub(crate) async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        let status = self.child_mut().wait().await;
        if status.is_ok() {
            self.child_reaped = true;
        }
        status
    }

    /// Kills the process group, reaps the direct child, and disarms the guard.
    pub(crate) async fn terminate(&mut self) {
        self.armed = false;
        signal_group(self.pgid);
        if !self.child_reaped
            && let Some(mut child) = self.child.take()
        {
            #[cfg(not(unix))]
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
    }

    /// Disarms the guard after the child and inherited pipes are fully drained.
    pub(crate) fn disarm(&mut self) {
        debug_assert!(self.child_reaped);
        self.armed = false;
        self.child.take();
    }
}

impl Drop for ScannerProcessGroup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        signal_group(self.pgid);
        if self.child_reaped {
            return;
        }
        let Some(mut child) = self.child.take() else {
            return;
        };
        #[cfg(not(unix))]
        let _ = child.start_kill();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = child.wait().await;
            });
        } else {
            let _ = child.start_kill();
        }
    }
}

#[cfg(unix)]
fn signal_group(pgid: i32) {
    // SAFETY: `killpg` does not dereference pointers or transfer ownership.
    // `pgid` is the positive PID returned for the child that was configured
    // with `process_group(0)` before spawn. ESRCH means the group already exited.
    let result = unsafe { libc::killpg(pgid, libc::SIGKILL) };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            warn!(pgid, %error, "failed to terminate scanner process group");
        }
    }
}

#[cfg(not(unix))]
fn signal_group(_pgid: i32) {}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    fn process_is_alive(pid: i32) -> bool {
        // SAFETY: Signal 0 performs only an existence/permission check and does
        // not dereference pointers. The test PID is parsed from the child fixture.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    async fn wait_for_pid(path: &std::path::Path) -> i32 {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(value) = tokio::fs::read_to_string(path).await
                    && let Ok(pid) = value.trim().parse()
                {
                    return pid;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("descendant PID should be published")
    }

    async fn wait_for_exit(pid: i32) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while process_is_alive(pid) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("descendant must be killed and reaped");
    }

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let directory = tempfile::tempdir().expect("process-group fixture directory");
        let script = directory.path().join("spawn-descendant");
        std::fs::write(&script, "#!/bin/sh\nsleep 60 &\necho $! > \"$1\"\nwait\n")
            .expect("fixture script");
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).unwrap();
        (directory, script)
    }

    async fn spawn_fixture(script: &std::path::Path, pid_file: &std::path::Path) -> Child {
        for attempt in 0..3 {
            let mut command = Command::new("sh");
            command.arg(script).arg(pid_file);
            isolate(&mut command);
            match command.spawn() {
                Ok(child) => return child,
                Err(error) if error.raw_os_error() == Some(libc::ETXTBSY) && attempt < 2 => {
                    // The Nix sandbox can briefly report ETXTBSY while it prepares
                    // executables for concurrent tests. Retry only that transient error.
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(error) => panic!("fixture should spawn: {error}"),
            }
        }
        unreachable!("the retry loop returns or panics on every final attempt")
    }

    #[tokio::test]
    async fn explicit_termination_kills_descendant_group_and_reaps_leader() {
        let (directory, script) = fixture();
        let pid_file = directory.path().join("terminate.pid");
        let child = spawn_fixture(&script, &pid_file).await;
        let mut group = ScannerProcessGroup::new(child, "fixture").unwrap();
        let descendant = wait_for_pid(&pid_file).await;
        group.terminate().await;
        wait_for_exit(descendant).await;
    }

    #[tokio::test]
    async fn guard_drop_kills_descendant_group_during_future_cancellation() {
        let (directory, script) = fixture();
        let pid_file = directory.path().join("drop.pid");
        let child = spawn_fixture(&script, &pid_file).await;
        let group = ScannerProcessGroup::new(child, "fixture").unwrap();
        let descendant = wait_for_pid(&pid_file).await;
        drop(group);
        wait_for_exit(descendant).await;
    }
}
