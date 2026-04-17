//! Global application state shared across views via Dioxus context.

use dioxus::prelude::*;

use crate::api::models::{AuthContext, ConfigHealthResponse};

/// State of authentication fetch operation.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum AuthFetchState {
    /// Auth context has not been fetched yet.
    #[default]
    Loading,
    /// Auth context was successfully fetched.
    Loaded,
    /// Auth fetch failed (network error, server error, etc.)
    Error,
}

/// State of shared config-health fetch operation.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ConfigHealthFetchState {
    #[default]
    Idle,
    Loading,
    Loaded,
    Error,
}

/// Global application configuration and shared state.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Base URL for the Crystal Forge API (e.g. "http://localhost:3000/api/v1").
    pub api_base_url: String,
    /// Polling interval for dashboard data in seconds.
    pub poll_interval_secs: u64,
    /// Current authentication context (None if not authenticated or not yet loaded).
    pub auth: Option<AuthContext>,
    /// State of auth fetch operation.
    pub auth_fetch_state: AuthFetchState,
    /// Shared admin config-health response.
    pub config_health: Option<ConfigHealthResponse>,
    /// State of the shared config-health fetch.
    pub config_health_fetch_state: ConfigHealthFetchState,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            api_base_url: "/api/v1".to_string(),
            poll_interval_secs: 30,
            auth: None,
            auth_fetch_state: AuthFetchState::Loading,
            config_health: None,
            config_health_fetch_state: ConfigHealthFetchState::Idle,
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
