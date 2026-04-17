//! Authentication bootstrap for Crystal Forge Web UI.
//!
//! Handles fetching and hydrating the auth context on application startup.

use crate::api::client;
use crate::state::app_state::{AppState, AuthFetchState};

/// Check if UI check mock auth mode is enabled via query param.
/// Only available in debug builds to prevent production auth bypass.
#[cfg(debug_assertions)]
pub fn ui_check_mock_auth_enabled() -> bool {
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .map(|q| q.contains("ui_check_auth=1"))
        .unwrap_or(false)
}

#[cfg(not(debug_assertions))]
pub fn ui_check_mock_auth_enabled() -> bool {
    false
}

/// Initialize authentication context for the application.
///
/// This function fetches the current user's auth context from the server
/// and updates the app state accordingly. It handles both production auth
/// and mock auth for development/testing purposes.
pub fn init_auth(mut app_state: dioxus::prelude::Signal<AppState>) {
    use dioxus::prelude::*;

    use_effect(move || {
        spawn(async move {
            // Skip API call if mock auth is enabled (for screenshot tests in debug builds)
            if ui_check_mock_auth_enabled() {
                let mut state = app_state.write();
                state.auth_fetch_state = AuthFetchState::Loaded;
                return;
            }
            match client::fetch_whoami().await {
                Ok(auth_context) => {
                    let mut state = app_state.write();
                    state.auth = Some(auth_context);
                    state.auth_fetch_state = AuthFetchState::Loaded;
                }
                Err(_) => {
                    app_state.write().auth_fetch_state = AuthFetchState::Error;
                }
            }
        });
    });
}
