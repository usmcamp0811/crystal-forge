//! Global application state shared across views via Dioxus context.

use dioxus::prelude::*;

use crate::alerts::set_current_user_id;
use crate::api::models::{AuthContext, ClassificationBannerConfig, ConfigHealthResponse};

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
    /// Monotonic generation bumped whenever the authenticated session boundary changes.
    pub auth_generation: u64,
    /// Shared admin config-health response.
    pub config_health: Option<ConfigHealthResponse>,
    /// State of the shared config-health fetch.
    pub config_health_fetch_state: ConfigHealthFetchState,
    /// Persisted classification banner configuration (None until loaded).
    pub classification_config: Option<ClassificationBannerConfig>,
    /// Tracks whether the classification config fetch has been attempted.
    /// None = not yet attempted, Some(Ok(())) = succeeded, Some(Err(msg)) = failed.
    pub classification_fetch_state: Option<Result<(), String>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            api_base_url: "/api/v1".to_string(),
            poll_interval_secs: 30,
            auth: None,
            auth_fetch_state: AuthFetchState::Loading,
            auth_generation: 0,
            config_health: None,
            config_health_fetch_state: ConfigHealthFetchState::Idle,
            classification_config: None,
            classification_fetch_state: None,
        }
    }
}

/// Store a fresh authenticated context and bump the auth generation.
pub fn set_authenticated_context(state: &mut AppState, auth_context: AuthContext) {
    if let Some(user) = &auth_context.user {
        set_current_user_id(&user.id);
    }
    state.auth = Some(auth_context);
    state.auth_fetch_state = AuthFetchState::Loaded;
    state.auth_generation = state.auth_generation.saturating_add(1);
}

/// Clear account-scoped authentication state and bump the auth generation.
pub fn clear_authenticated_context(state: &mut AppState) {
    set_current_user_id("");
    state.auth = None;
    state.auth_fetch_state = AuthFetchState::Loaded;
    state.auth_generation = state.auth_generation.saturating_add(1);
}

/// Provide the global [`AppState`] as a Dioxus context.
///
/// Call this once in the root component so child components can
/// access it via `use_context::<Signal<AppState>>()`.
pub fn provide_app_state() {
    use_context_provider(|| Signal::new(AppState::default()));
}
