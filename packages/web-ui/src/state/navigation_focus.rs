//! Lightweight cross-view focus state for deep-link navigation.

use dioxus::prelude::*;

/// Target view for a cross-surface focus request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusTarget {
    /// Builds view focus.
    #[default]
    Builds,
}

/// Shared navigation focus payload.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NavigationFocus {
    pub target: FocusTarget,
    pub commit_sha: Option<String>,
    pub flake_name: Option<String>,
    pub status: Option<String>,
}

/// Provide the global navigation focus signal.
pub fn provide_navigation_focus() {
    use_context_provider(|| Signal::new(None::<NavigationFocus>));
}
