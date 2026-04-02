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
/// - `git+https://...#nixosConfigurations.gray` → `gray`
/// - `gray` → `gray`
pub fn extract_system_name(hostname: &str) -> &str {
    // If there's a # (flake attribute path), extract everything after it
    if let Some(attr_path) = hostname.split('#').nth(1) {
        // Split by dots and take the last segment (the actual system name)
        // Examples:
        //   nixosConfigurations.test.gray -> gray
        //   nixosConfigurations.gray -> gray
        attr_path.split('.').last().unwrap_or(attr_path)
    } else {
        // No flake path, just return the hostname as-is
        hostname
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
    Stopping,
    Restarting,
    Failed,
    Complete,
    Canceled,
}

impl BuildStatus {
    pub fn label(self) -> &'static str {
        match self {
            BuildStatus::Queued => "queued",
            BuildStatus::Building => "building",
            BuildStatus::Stopping => "stopping",
            BuildStatus::Restarting => "restarting",
            BuildStatus::Failed => "failed",
            BuildStatus::Complete => "complete",
            BuildStatus::Canceled => "canceled",
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
    Restart,
    RunNext,
}

/// Worker item struct.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkerItem {
    pub id: String,
    pub name: String,
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
    pub worker_id: String,
    pub queued_for: String,
    pub runtime: Option<String>,
    pub duration_secs: Option<i64>,
    pub completed_at: Option<DateTime<Utc>>,
    pub started_by: String,
    /// Stored build logs (available for completed/failed jobs from API).
    pub logs: Option<String>,
    pub status: BuildStatus,
    pub summary: String,
}

impl BuildItem {
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
        build_id: i32,
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
        BuildStatus::Restarting => "cf-build-status-restarting",
        BuildStatus::Failed => "cf-build-status-failed",
        BuildStatus::Complete => "cf-build-status-complete",
        BuildStatus::Canceled => "cf-build-status-canceled",
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

pub fn queue_row_style(selected: bool, status: BuildStatus) -> String {
    let row_status = match status {
        BuildStatus::Building => "cf-queue-row-building",
        BuildStatus::Restarting => "cf-queue-row-restarting",
        BuildStatus::Queued => "cf-queue-row-queued",
        BuildStatus::Stopping => "cf-queue-row-stopping",
        BuildStatus::Failed => "cf-queue-row-failed",
        BuildStatus::Complete => "cf-queue-row-complete",
        BuildStatus::Canceled => "cf-queue-row-canceled",
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
    selected_build: &mut Signal<Option<i32>>,
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
        PendingAction::Build { build_id, action } => {
            let mut next_builds = builds.read().clone();
            match action {
                BuildAction::Stop => {
                    if let Some(target) = next_builds.iter_mut().find(|b| b.id == build_id) {
                        target.status = BuildStatus::Stopping;
                    }
                    if let Some(target) = next_builds.iter_mut().find(|b| b.id == build_id) {
                        target.status = BuildStatus::Canceled;
                    }
                    note.set(Some(format!("Stopped build #{build_id}")));
                }
                BuildAction::Restart => {
                    if let Some(target) = next_builds.iter_mut().find(|b| b.id == build_id) {
                        target.status = BuildStatus::Restarting;
                        target.runtime = Some("00:00".to_string());
                        target.queued_for = "restarting".to_string();
                    }
                    if let Some(target) = next_builds.iter_mut().find(|b| b.id == build_id) {
                        target.status = BuildStatus::Building;
                    }
                    note.set(Some(format!("Restarted build #{build_id}")));
                }
                BuildAction::RunNext => {
                    if let Some(index) = next_builds.iter().position(|b| b.id == build_id) {
                        let target = next_builds.remove(index);
                        let insert_idx = next_builds
                            .iter()
                            .position(|b| b.status == BuildStatus::Queued)
                            .unwrap_or(next_builds.len());
                        next_builds.insert(insert_idx, target);
                        selected_build.set(Some(build_id));
                        note.set(Some(format!("Prioritized build #{build_id}")));
                    }
                }
            }
            builds.set(next_builds);
        }
    }
}

pub fn selected_build_data(selected_id: Option<i32>, builds: &[BuildItem]) -> Option<BuildItem> {
    if let Some(id) = selected_id {
        builds.iter().find(|b| b.id == id).cloned()
    } else {
        builds.first().cloned()
    }
}

// Mock data functions

pub fn mock_workers() -> Vec<WorkerItem> {
    vec![
        WorkerItem {
            id: "worker-a".to_string(),
            name: "worker-a".to_string(),
            active_slots: 2,
            total_slots: 4,
            queue_depth: 6,
            status: WorkerStatus::Running,
        },
        WorkerItem {
            id: "worker-b".to_string(),
            name: "worker-b".to_string(),
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
            worker_id: "worker-a".to_string(),
            queued_for: "queued 00:58 ago".to_string(),
            runtime: Some("02:13".to_string()),
            duration_secs: Some(133),
            completed_at: None,
            started_by: "mcamp".to_string(),
            logs: None,
            status: BuildStatus::Building,
            summary: "nix build .#nixosConfigurations.atlas-01.config.system.build.toplevel"
                .to_string(),
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
            worker_id: "worker-b".to_string(),
            queued_for: "queued 01:32 ago".to_string(),
            runtime: None,
            duration_secs: None,
            completed_at: None,
            started_by: "scheduler".to_string(),
            logs: None,
            status: BuildStatus::Queued,
            summary: "waiting for free worker slot".to_string(),
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
            worker_id: "worker-a".to_string(),
            queued_for: "queued 00:29 ago".to_string(),
            runtime: None,
            duration_secs: None,
            completed_at: None,
            started_by: "scheduler".to_string(),
            logs: None,
            status: BuildStatus::Queued,
            summary: "waiting for free worker slot".to_string(),
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
            worker_id: "worker-b".to_string(),
            queued_for: "queued 06:11 ago".to_string(),
            runtime: Some("04:22".to_string()),
            duration_secs: Some(262),
            completed_at: None,
            started_by: "mcamp".to_string(),
            logs: None,
            status: BuildStatus::Failed,
            summary: "dependency graph diverged on nixpkgs input".to_string(),
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
