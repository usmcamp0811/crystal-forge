//! Collects only server-associated MicroVM unit evidence during normal heartbeats.
//! No discovery, lifecycle operations, or guest closure inference occurs here.

use cf_protocol::agent::{GuestObservation, GuestObservationTarget};
use chrono::Utc;
use tokio::{
    process::Command,
    time::{Duration, timeout},
};

pub(super) async fn collect(targets: &[GuestObservationTarget]) -> Vec<GuestObservation> {
    let mut observations = Vec::new();
    for target in targets.iter().filter(|target| target.is_valid()) {
        // SECURITY: Never interpret server data as options or shell commands.
        // A bounded read cannot indefinitely block the parent's heartbeat.
        let mut command = Command::new("systemctl");
        command.args([
            "show",
            "--no-pager",
            "--property=ActiveState",
            "--",
            &target.unit,
        ]);
        command.kill_on_drop(true);
        let unit_active_state = match timeout(Duration::from_secs(5), command.output()).await {
            Ok(Ok(output)) if output.status.success() => parse_active_state(&output.stdout),
            _ => None,
        };
        observations.push(GuestObservation {
            target: target.clone(),
            unit_active_state,
            // ActiveState does not establish an authoritative runner executable.
            // In particular, arbitrary ExecStart arguments are not guest closures.
            runner_store_path: None,
            observed_at: Utc::now(),
        });
    }
    observations
}

fn parse_active_state(output: &[u8]) -> Option<String> {
    let output = std::str::from_utf8(output).ok()?;
    let state = output.trim().strip_prefix("ActiveState=")?;
    (!state.is_empty()
        && state.len() <= 64
        && state.bytes().all(|c| c.is_ascii_lowercase() || c == b'-'))
    .then(|| state.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_parses_only_active_state_not_command_or_closure_output() {
        assert_eq!(
            parse_active_state(b"ActiveState=active\n").as_deref(),
            Some("active")
        );
        assert_eq!(
            parse_active_state(b"ActiveState=failed\n").as_deref(),
            Some("failed")
        );
        for output in [
            b"".as_slice(),
            b"ActiveState=",
            b"active",
            b"ActiveState=active\nExecStart=/nix/store/pretend",
            b"ActiveState=not active",
        ] {
            assert!(parse_active_state(output).is_none());
        }
    }

    #[tokio::test]
    async fn invalid_targets_never_collect() {
        let target: GuestObservationTarget = serde_json::from_value(serde_json::json!({
            "guest_id":"00000000-0000-0000-0000-000000000001",
            "guest_identity":"guest", "unit":"sshd.service"
        }))
        .unwrap();
        assert!(collect(&[target]).await.is_empty());
    }
}
