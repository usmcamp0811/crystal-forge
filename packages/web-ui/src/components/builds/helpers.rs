//! Shared types and helper functions for build components.

use dioxus::prelude::*;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use web_sys::{Node, window};

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
    pub id: &'static str,
    pub name: &'static str,
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
    pub hostname: &'static str,
    pub flake: &'static str,
    pub commit: &'static str,
    pub branch: &'static str,
    pub worker_id: &'static str,
    pub queued_for: &'static str,
    pub runtime: Option<&'static str>,
    pub started_by: &'static str,
    pub status: BuildStatus,
    pub summary: &'static str,
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
        worker_id: &'static str,
        action: WorkerAction,
    },
    Build {
        build_id: i32,
        action: BuildAction,
    },
}

// Styling helper functions

pub fn worker_status_style(status: WorkerStatus) -> &'static str {
    match status {
        WorkerStatus::Running => {
            "background-color: #1E3A2E; border-color: #2F6B4A; color: #D8FBE8;"
        }
        WorkerStatus::Paused => "background-color: #2B303B; border-color: #495264; color: #E5E7EB;",
        WorkerStatus::Draining => {
            "background-color: #4A3B22; border-color: #8C6A2F; color: #FDE8C6;"
        }
    }
}

pub fn build_status_badge_style(status: BuildStatus) -> &'static str {
    match status {
        BuildStatus::Queued => "background-color: #2E2E3F; border-color: #4D4D72; color: #D9D9FF;",
        BuildStatus::Building => {
            "background-color: #23363A; border-color: #3D6870; color: #D9F6F9;"
        }
        BuildStatus::Stopping => {
            "background-color: #4A3B22; border-color: #8C6A2F; color: #FDE8C6;"
        }
        BuildStatus::Restarting => {
            "background-color: #2E2A49; border-color: #675CAD; color: #E4DFFF;"
        }
        BuildStatus::Failed => "background-color: #44262A; border-color: #7A3D48; color: #FFDCE1;",
        BuildStatus::Complete => {
            "background-color: #1E3A2E; border-color: #2F6B4A; color: #D8FBE8;"
        }
        BuildStatus::Canceled => {
            "background-color: #2B303B; border-color: #495264; color: #E5E7EB;"
        }
    }
}

pub fn event_level_style(level: &str) -> &'static str {
    match level {
        "error" => "background-color: #44262A; border-color: #7A3D48; color: #FFDCE1;",
        "warn" => "background-color: #4A3B22; border-color: #8C6A2F; color: #FDE8C6;",
        _ => "background-color: #2B303B; border-color: #495264; color: #E5E7EB;",
    }
}

pub fn queue_sort_rank(status: BuildStatus) -> i32 {
    match status {
        BuildStatus::Building | BuildStatus::Restarting => 0,
        BuildStatus::Queued => 1,
        BuildStatus::Stopping => 2,
        BuildStatus::Failed => 3,
        BuildStatus::Complete => 4,
        BuildStatus::Canceled => 5,
    }
}

pub fn short_commit(commit: &str) -> String {
    commit.chars().take(7).collect()
}

pub fn queue_row_style(selected: bool, status: BuildStatus) -> String {
    let border = if selected { "#6D8FBA" } else { "#374151" };
    let bg = match status {
        BuildStatus::Building | BuildStatus::Restarting => "#1C2B3E",
        BuildStatus::Queued => "#242C3A",
        BuildStatus::Stopping => "#3C2F20",
        BuildStatus::Failed => "#3B232A",
        BuildStatus::Complete => "#1E362E",
        BuildStatus::Canceled => "#2C313A",
    };

    format!("background-color: {bg}; border-color: {border};")
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
                        target.runtime = Some("00:00");
                        target.queued_for = "restarting";
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
            id: "worker-a",
            name: "worker-a",
            active_slots: 2,
            total_slots: 4,
            queue_depth: 6,
            status: WorkerStatus::Running,
        },
        WorkerItem {
            id: "worker-b",
            name: "worker-b",
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
            hostname: "atlas-01",
            flake: "campground",
            commit: "a38f45fba91d4b0a5d80840c09b0910c70fa013e",
            branch: "main",
            worker_id: "worker-a",
            queued_for: "queued 00:58 ago",
            runtime: Some("02:13"),
            started_by: "mcamp",
            status: BuildStatus::Building,
            summary: "nix build .#nixosConfigurations.atlas-01.config.system.build.toplevel",
        },
        BuildItem {
            id: 2,
            hostname: "luna-02",
            flake: "campground",
            commit: "75c2fbf719ac2654af9f1dc4b773f502f9db515e",
            branch: "main",
            worker_id: "worker-b",
            queued_for: "queued 01:32 ago",
            runtime: None,
            started_by: "scheduler",
            status: BuildStatus::Queued,
            summary: "waiting for free worker slot",
        },
        BuildItem {
            id: 3,
            hostname: "gray",
            flake: "campground",
            commit: "4144fdc0312734c62bc5f4f9f48f5a87e4b3a85f",
            branch: "main",
            worker_id: "worker-a",
            queued_for: "queued 00:29 ago",
            runtime: None,
            started_by: "scheduler",
            status: BuildStatus::Queued,
            summary: "waiting for free worker slot",
        },
        BuildItem {
            id: 4,
            hostname: "reckless",
            flake: "campground",
            commit: "9cc53a8f1792043b1f7868ecf5ff312ad67553de",
            branch: "release/2026-02",
            worker_id: "worker-b",
            queued_for: "queued 06:11 ago",
            runtime: Some("04:22"),
            started_by: "mcamp",
            status: BuildStatus::Failed,
            summary: "dependency graph diverged on nixpkgs input",
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
