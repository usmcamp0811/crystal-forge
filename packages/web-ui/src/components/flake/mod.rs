//! Flake-specific UI components.
//!
//! Components for displaying and interacting with flake data,
//! including flake cards, history explorers, and diff viewers.

pub mod flake_timeline;
pub mod sync_chip;
pub mod sync_error_banner;

pub use flake_timeline::FlakeTimelineWidget;
pub use sync_chip::FlakeSyncChip;
pub use sync_error_banner::FlakeSyncErrorBanner;

// TODO: Extract more flake components from flakes_list.rs
// - FlakeCard
// - FlakeHistoryExplorer
// - FriendlyDiffViewer
