//! Build-related UI components.
//!
//! Components for the builds control center, including
//! worker strips, build queue panes, and build detail views.

mod build_detail_pane;
mod build_queue_pane;
mod helpers;
mod metrics_row;
mod worker_strip;

pub use build_detail_pane::{BuildDetailPane, ConfirmActionModal, DetailTab, QueueActionButton};
pub use build_queue_pane::BuildQueuePane;
pub use helpers::{
    apply_action, extract_system_name, mock_artifacts, mock_builds, mock_events, mock_logs,
    mock_workers, selected_build_data, BuildAction, BuildArtifact, BuildEvent, BuildItem,
    BuildStatus, PendingAction, QueueAction, WorkerAction, WorkerItem, WorkerStatus,
};
pub use metrics_row::MetricsRow;
pub use worker_strip::WorkerStrip;
