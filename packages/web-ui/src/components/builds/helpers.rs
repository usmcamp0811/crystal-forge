//! Shared types and helper functions for build components.

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use web_sys::{Node, window};

/// Extract the system name from a full flake attribute path or hostname.
///
/// Examples:
/// - `git+https://...#nixosConfigurations.test.gray` → `gray`
/// - `nixosConfigurations.test.gray` → `gray`
/// - `nixosConfigurations.gray` → `gray`
/// - `gray` → `gray`
pub fn extract_system_name(hostname: &str) -> &str {
    // Split on # to handle full flake refs, use everything after # or the whole string
    let attr_path = hostname
        .rsplit_once('#')
        .map(|(_, attr_path)| attr_path)
        .unwrap_or(hostname);

    // If this is a nixosConfigurations path, extract the last segment
    if attr_path.starts_with("nixosConfigurations.") {
        attr_path.rsplit('.').next().unwrap_or(attr_path)
    } else {
        attr_path
    }
}

/// Worker status enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerStatus {
    Running,
    Paused,
    Draining,
}

impl WorkerStatus {
    pub fn label(self) -> &'static str {
        match self {
            WorkerStatus::Running => "running",
            WorkerStatus::Paused => "paused",
            WorkerStatus::Draining => "draining",
        }
    }
}

/// Build status enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildStatus {
    Queued,
    Building,
    /// Cancel requested; builder is stopping the nix process.
    Stopping,
    Failed,
    Complete,
    Cancelled,
}

impl BuildStatus {
    // JSX: BUILD_STATUS_META labels are Title Case (data-builds.js): "Queued",
    // "Building", "Stopping", "Complete", "Failed", "Cancelled".
    pub fn label(self) -> &'static str {
        match self {
            BuildStatus::Queued => "Queued",
            BuildStatus::Building => "Building",
            BuildStatus::Stopping => "Stopping",
            BuildStatus::Failed => "Failed",
            BuildStatus::Complete => "Complete",
            BuildStatus::Cancelled => "Cancelled",
        }
    }
}

/// Queue action enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueAction {
    StartAll,
    PauseAll,
    DrainAll,
}

impl QueueAction {
    pub fn label(self) -> &'static str {
        match self {
            QueueAction::StartAll => "start all",
            QueueAction::PauseAll => "pause all",
            QueueAction::DrainAll => "drain all",
        }
    }
}

/// Worker action enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerAction {
    Start,
    Pause,
    Drain,
}

impl WorkerAction {
    pub fn label(self) -> &'static str {
        match self {
            WorkerAction::Start => "start",
            WorkerAction::Pause => "pause",
            WorkerAction::Drain => "drain",
        }
    }
}

/// Build action enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildAction {
    Stop,
    ForceCancel,
    Restart,
    RunNext,
    MoveUp,
    MoveDown,
}

/// Worker item struct.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkerItem {
    pub id: String,
    pub name: String,
    pub host: Option<String>,
    pub arch: Option<String>,
    pub cpu_cores: Option<i32>,
    pub memory_gb: Option<i32>,
    pub active_slots: usize,
    pub total_slots: usize,
    pub queue_depth: usize,
    pub status: WorkerStatus,
}

impl WorkerItem {
    pub fn status_label(&self) -> &'static str {
        self.status.label()
    }
}

/// Build item struct.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildItem {
    pub id: i32,
    pub job_id: Option<uuid::Uuid>,
    pub system_id: Option<uuid::Uuid>,
    pub hostname: String,
    pub environment: Option<String>,
    pub flake: String,
    pub commit: String,
    pub branch: String,
    pub arch: String,
    pub worker_id: String,
    pub queued_at: DateTime<Utc>,
    pub queued_for: String,
    pub runtime: Option<String>,
    pub duration_secs: Option<i64>,
    pub completed_at: Option<DateTime<Utc>>,
    pub started_by: String,
    /// Stored build logs (available for completed/failed jobs from API).
    pub logs: Option<String>,
    pub status: BuildStatus,
    pub summary: String,
    // Derivation progress fields (matching JSX b.cachedDerivs / b.builtDerivs / b.totalDerivs)
    pub cached_derivs: usize,
    pub built_derivs: usize,
    pub total_derivs: usize,
    /// Currently building package path (shown for active builds).
    pub current_pkg: Option<String>,
    /// Package that failed (shown for failed builds).
    pub failed_pkg: Option<String>,
    /// Number of build attempts.
    pub attempts: usize,
}

/// Helper methods for BuildItem display.
impl BuildItem {
    /// Package name for display (JSX: b.pkg).
    /// Extracts clean system name from flake attribute path (e.g., "daly" from "nixosConfigurations.daly").
    pub fn pkg(&self) -> String {
        extract_system_name(&self.hostname).to_string()
    }

    /// Derivation path for display (JSX: b.drv).
    /// Synthesizes a Nix store path using commit hash and clean system name.
    pub fn drv(&self) -> String {
        // Format: /nix/store/{hash_prefix}-nixos-system-{clean_name}.drv
        // Use first 11 chars of commit to create plausible store hash prefix
        let hash_prefix = if self.commit.len() >= 11 {
            &self.commit[..11]
        } else {
            &self.commit
        };
        let clean_name = extract_system_name(&self.hostname);
        format!("/nix/store/{}-nixos-system-{}.drv", hash_prefix, clean_name)
    }

    /// Status label for display.
    pub fn status_label(&self) -> &'static str {
        self.status.label()
    }
}

/// Build event struct.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildEvent {
    pub ts: &'static str,
    pub level: &'static str,
    pub message: &'static str,
}

/// Build artifact struct.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildArtifact {
    pub name: &'static str,
    pub size: &'static str,
    pub hash: &'static str,
}

/// Pending action enum.
#[derive(Clone, Debug, PartialEq)]
pub enum PendingAction {
    Queue(QueueAction),
    Worker {
        worker_id: String,
        action: WorkerAction,
    },
    Build {
        job_id: uuid::Uuid,
        action: BuildAction,
    },
}

// Styling helper functions

pub fn worker_status_class(status: WorkerStatus) -> &'static str {
    match status {
        WorkerStatus::Running => "cf-worker-status-running",
        WorkerStatus::Paused => "cf-worker-status-paused",
        WorkerStatus::Draining => "cf-worker-status-draining",
    }
}

pub fn build_status_badge_class(status: BuildStatus) -> &'static str {
    match status {
        BuildStatus::Queued => "cf-build-status-queued",
        BuildStatus::Building => "cf-build-status-building",
        BuildStatus::Stopping => "cf-build-status-stopping",
        BuildStatus::Failed => "cf-build-status-failed",
        BuildStatus::Complete => "cf-build-status-complete",
        BuildStatus::Cancelled => "cf-build-status-canceled",
    }
}

pub fn event_level_class(level: &str) -> &'static str {
    match level {
        "error" => "cf-event-level-error",
        "warn" => "cf-event-level-warn",
        "info" => "cf-event-level-info",
        _ => "cf-event-level-default",
    }
}

pub fn short_commit(commit: &str) -> String {
    commit.chars().take(7).collect()
}

/// Sort rank for queue ordering: lower = displayed first.
pub fn queue_sort_rank(status: BuildStatus) -> i32 {
    match status {
        BuildStatus::Building => 0,
        BuildStatus::Stopping => 1,
        BuildStatus::Queued => 2,
        BuildStatus::Failed => 3,
        BuildStatus::Complete => 4,
        BuildStatus::Cancelled => 5,
    }
}

pub fn queue_row_style(selected: bool, status: BuildStatus) -> String {
    let row_status = match status {
        BuildStatus::Building => "cf-queue-row-building",
        BuildStatus::Queued => "cf-queue-row-queued",
        BuildStatus::Stopping => "cf-queue-row-stopping",
        BuildStatus::Failed => "cf-queue-row-failed",
        BuildStatus::Complete => "cf-queue-row-complete",
        BuildStatus::Cancelled => "cf-queue-row-canceled",
    };
    let selected_class = if selected {
        "cf-queue-row-selected"
    } else {
        "cf-queue-row"
    };
    format!("{selected_class} {row_status}")
}

// Action application functions

pub fn apply_action(
    action: PendingAction,
    workers: &mut Signal<Vec<WorkerItem>>,
    builds: &mut Signal<Vec<BuildItem>>,
    selected_build: &mut Signal<Option<uuid::Uuid>>,
    note: &mut Signal<Option<String>>,
) {
    match action {
        PendingAction::Queue(queue_action) => {
            let mut next_workers = workers.read().clone();
            for worker in &mut next_workers {
                worker.status = match queue_action {
                    QueueAction::StartAll => WorkerStatus::Running,
                    QueueAction::PauseAll => WorkerStatus::Paused,
                    QueueAction::DrainAll => WorkerStatus::Draining,
                };
            }
            workers.set(next_workers);
            note.set(Some(format!("Applied {}", queue_action.label())));
        }
        PendingAction::Worker { worker_id, action } => {
            let mut next_workers = workers.read().clone();
            if let Some(worker) = next_workers.iter_mut().find(|w| w.id == worker_id) {
                worker.status = match action {
                    WorkerAction::Start => WorkerStatus::Running,
                    WorkerAction::Pause => WorkerStatus::Paused,
                    WorkerAction::Drain => WorkerStatus::Draining,
                };
            }
            workers.set(next_workers);
            note.set(Some(format!("Applied {} on {worker_id}", action.label())));
        }
        PendingAction::Build { job_id, action } => {
            let mut next_builds = builds.read().clone();
            match action {
                BuildAction::Stop => {
                    // Optimistic UI: show Stopping immediately; server will confirm.
                    if let Some(target) = next_builds.iter_mut().find(|b| b.job_id == Some(job_id))
                    {
                        target.status = BuildStatus::Stopping;
                    }
                    if let Some(target) = next_builds.iter_mut().find(|b| b.job_id == Some(job_id))
                    {
                        target.status = BuildStatus::Cancelled;
                    }
                    note.set(Some(format!("Stopped build {job_id}")));
                }
                BuildAction::ForceCancel => {
                    // Optimistic UI: show Cancelled immediately; server will confirm.
                    if let Some(target) = next_builds.iter_mut().find(|b| b.job_id == Some(job_id))
                    {
                        target.status = BuildStatus::Cancelled;
                    }
                    note.set(Some(format!("Force-cancelled build {job_id}")));
                }
                BuildAction::Restart => {
                    // Optimistic UI: show Queued immediately; server will confirm.
                    if let Some(target) = next_builds.iter_mut().find(|b| b.job_id == Some(job_id))
                    {
                        target.status = BuildStatus::Building;
                        target.runtime = Some("00:00".to_string());
                        target.queued_for = "restarting".to_string();
                    }
                    note.set(Some(format!("Restarted build {job_id}")));
                }
                BuildAction::RunNext => {
                    if let Some(index) = next_builds.iter().position(|b| b.job_id == Some(job_id)) {
                        let target = next_builds.remove(index);
                        let insert_idx = next_builds
                            .iter()
                            .position(|b| b.status == BuildStatus::Queued)
                            .unwrap_or(next_builds.len());
                        next_builds.insert(insert_idx, target);
                        selected_build.set(Some(job_id));
                        note.set(Some(format!("Prioritized build {job_id}")));
                    }
                }
                BuildAction::MoveUp | BuildAction::MoveDown => {
                    // Persistent queue reorder is handled through backend endpoints in builds.rs.
                }
            }
            builds.set(next_builds);
        }
    }
}

pub fn selected_build_data(
    selected_id: Option<uuid::Uuid>,
    builds: &[BuildItem],
) -> Option<BuildItem> {
    if let Some(id) = selected_id {
        builds.iter().find(|b| b.job_id == Some(id)).cloned()
    } else {
        // JSX: selected defaults to null, not first build
        None
    }
}

// Mock data functions

pub fn mock_workers() -> Vec<WorkerItem> {
    vec![
        WorkerItem {
            id: "worker-a".to_string(),
            name: "worker-a".to_string(),
            host: Some("worker-a.lab".to_string()),
            arch: Some("x86_64-linux".to_string()),
            cpu_cores: Some(16),
            memory_gb: Some(64),
            active_slots: 2,
            total_slots: 4,
            queue_depth: 6,
            status: WorkerStatus::Running,
        },
        WorkerItem {
            id: "worker-b".to_string(),
            name: "worker-b".to_string(),
            host: Some("worker-b.lab".to_string()),
            arch: Some("x86_64-linux".to_string()),
            cpu_cores: Some(16),
            memory_gb: Some(64),
            active_slots: 3,
            total_slots: 4,
            queue_depth: 4,
            status: WorkerStatus::Running,
        },
    ]
}

pub fn mock_builds() -> Vec<BuildItem> {
    vec![
        BuildItem {
            id: 1,
            job_id: None,
            system_id: None,
            hostname: "atlas-01".to_string(),
            environment: Some("production".to_string()),
            flake: "campground".to_string(),
            commit: "a38f45fba91d4b0a5d80840c09b0910c70fa013e".to_string(),
            branch: "main".to_string(),
            arch: "x86_64-linux".to_string(),
            worker_id: "worker-a".to_string(),
            queued_at: Utc::now(),
            queued_for: "queued 00:58 ago".to_string(),
            runtime: Some("02:13".to_string()),
            duration_secs: Some(133),
            completed_at: None,
            started_by: "mcamp".to_string(),
            logs: None,
            status: BuildStatus::Building,
            summary: "nix build .#nixosConfigurations.atlas-01.config.system.build.toplevel"
                .to_string(),
            cached_derivs: 42,
            built_derivs: 17,
            total_derivs: 120,
            current_pkg: Some("openssl-3.3.2".to_string()),
            failed_pkg: None,
            attempts: 1,
        },
        BuildItem {
            id: 2,
            job_id: None,
            system_id: None,
            hostname: "luna-02".to_string(),
            environment: Some("staging".to_string()),
            flake: "campground".to_string(),
            commit: "75c2fbf719ac2654af9f1dc4b773f502f9db515e".to_string(),
            branch: "main".to_string(),
            arch: "x86_64-linux".to_string(),
            worker_id: "worker-b".to_string(),
            queued_at: Utc::now(),
            queued_for: "queued 01:32 ago".to_string(),
            runtime: None,
            duration_secs: None,
            completed_at: None,
            started_by: "scheduler".to_string(),
            logs: None,
            status: BuildStatus::Queued,
            summary: "waiting for free worker slot".to_string(),
            cached_derivs: 0,
            built_derivs: 0,
            total_derivs: 0,
            current_pkg: None,
            failed_pkg: None,
            attempts: 1,
        },
        BuildItem {
            id: 3,
            job_id: None,
            system_id: None,
            hostname: "gray".to_string(),
            environment: Some("dev".to_string()),
            flake: "campground".to_string(),
            commit: "4144fdc0312734c62bc5f4f9f48f5a87e4b3a85f".to_string(),
            branch: "main".to_string(),
            arch: "x86_64-linux".to_string(),
            worker_id: "worker-a".to_string(),
            queued_at: Utc::now(),
            queued_for: "queued 00:29 ago".to_string(),
            runtime: None,
            duration_secs: None,
            completed_at: None,
            started_by: "scheduler".to_string(),
            logs: None,
            status: BuildStatus::Queued,
            summary: "waiting for free worker slot".to_string(),
            cached_derivs: 0,
            built_derivs: 0,
            total_derivs: 0,
            current_pkg: None,
            failed_pkg: None,
            attempts: 1,
        },
        BuildItem {
            id: 4,
            job_id: None,
            system_id: None,
            hostname: "reckless".to_string(),
            environment: Some("production".to_string()),
            flake: "campground".to_string(),
            commit: "9cc53a8f1792043b1f7868ecf5ff312ad67553de".to_string(),
            branch: "release/2026-02".to_string(),
            arch: "x86_64-linux".to_string(),
            worker_id: "worker-b".to_string(),
            queued_at: Utc::now(),
            queued_for: "queued 06:11 ago".to_string(),
            runtime: Some("04:22".to_string()),
            duration_secs: Some(262),
            completed_at: None,
            started_by: "mcamp".to_string(),
            logs: None,
            status: BuildStatus::Failed,
            summary: "dependency graph diverged on nixpkgs input".to_string(),
            cached_derivs: 38,
            built_derivs: 51,
            total_derivs: 97,
            current_pkg: None,
            failed_pkg: Some("nginx-1.27.4".to_string()),
            attempts: 2,
        },
    ]
}

pub fn mock_logs(build_id: i32) -> Vec<String> {
    let lines = match build_id {
        1 => vec![
            "[10:22:17] systemd[1]: Started crystal-forge-build@atlas-01.service",
            "[10:22:19] CF: reserving build slot worker-a/slot-2",
            "[10:22:22] nix: evaluating flake input graph...",
            "[10:22:26] nix: building /nix/store/5jg9...-kernel-modules.drv",
            "[10:22:31] nix: building /nix/store/qplm...-system-path.drv",
            "[10:22:35] nix: substituter cache hit ratio: 82%",
            "[10:22:41] nix: building /nix/store/nk2p...-etc.drv",
            "[10:22:44] nix: running post-build hooks",
            "[10:22:48] CF: build still running; heartbeat ok",
        ],
        2 => vec![
            "[10:21:02] CF: queued build request for luna-02",
            "[10:21:04] CF: assigned to worker-b queue",
            "[10:21:05] CF: waiting for available slot",
        ],
        3 => vec![
            "[10:21:39] CF: queued build request for gray",
            "[10:21:39] CF: waiting behind 1 queued item",
        ],
        _ => vec![
            "[10:19:11] systemd[1]: Started crystal-forge-build@reckless.service",
            "[10:19:15] nix: evaluating derivation graph",
            "[10:19:44] error: attribute 'myMissingPackage' missing",
            "[10:19:44] CF: build marked failed (exit code 1)",
        ],
    };

    lines.into_iter().map(|line| line.to_string()).collect()
}

pub fn mock_events(build_id: i32) -> Vec<BuildEvent> {
    match build_id {
        1 => vec![
            BuildEvent {
                ts: "10:22:17",
                level: "info",
                message: "Build unit started on worker-a",
            },
            BuildEvent {
                ts: "10:22:31",
                level: "info",
                message: "Substituter cache hit ratio reached 82%",
            },
            BuildEvent {
                ts: "10:22:48",
                level: "info",
                message: "Worker heartbeat healthy",
            },
        ],
        _ => vec![
            BuildEvent {
                ts: "10:19:11",
                level: "info",
                message: "Build unit started",
            },
            BuildEvent {
                ts: "10:19:44",
                level: "error",
                message: "Nix evaluation failed: missing attribute",
            },
            BuildEvent {
                ts: "10:19:44",
                level: "warn",
                message: "Build marked failed and removed from active worker slot",
            },
        ],
    }
}

pub fn mock_artifacts(build_id: i32) -> Vec<BuildArtifact> {
    match build_id {
        4 => vec![],
        _ => vec![
            BuildArtifact {
                name: "nixos-system-atlas-01-26.05.20260214.abc123",
                size: "1.3 GiB",
                hash: "sha256-4qkS4W+9Md0v9QY5B5hQmQ8wS6yupw7QmRGYH0xGm4Q=",
            },
            BuildArtifact {
                name: "closure-manifest.json",
                size: "18 KiB",
                hash: "sha256-csY0+fZq0xobLqD7zh9sPXoW3DkQMY8qv5cz4S9xRMo=",
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::extract_system_name;

    #[test]
    fn extracts_system_name_from_flake_attribute_paths() {
        assert_eq!(extract_system_name("nixosConfigurations.daly"), "daly");
        assert_eq!(extract_system_name("nixosConfigurations.test.gray"), "gray");
        assert_eq!(
            extract_system_name("git+https://example.test/repo#nixosConfigurations.test.gray"),
            "gray"
        );
        assert_eq!(extract_system_name("gray"), "gray");
    }
}
