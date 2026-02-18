//! Flake-specific UI components.
//!
//! Components for displaying and interacting with flake data,
//! including flake cards, history explorers, and diff viewers.

pub mod flake_timeline;

pub use flake_timeline::FlakeTimelineWidget;

// TODO: Extract more flake components from flakes_list.rs
// - FlakeCard
// - FlakeHistoryExplorer
// - FriendlyDiffViewer
