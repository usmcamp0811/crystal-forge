//! Global application state shared across views via Dioxus context.

use dioxus::prelude::*;

use crate::api::models::AuthContext;

/// Global application configuration and shared state.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Base URL for the Crystal Forge API (e.g. "http://localhost:3000/api/v1").
    pub api_base_url: String,
    /// Polling interval for dashboard data in seconds.
    pub poll_interval_secs: u64,
    /// Current authentication context (None if not yet loaded).
    pub auth: Option<AuthContext>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            api_base_url: "/api/v1".to_string(),
            poll_interval_secs: 30,
            auth: None,
        }
    }
}

/// Provide the global [`AppState`] as a Dioxus context.
///
/// Call this once in the root component so child components can
/// access it via `use_context::<Signal<AppState>>()`.
pub fn provide_app_state() {
    use_context_provider(|| Signal::new(AppState::default()));
}
