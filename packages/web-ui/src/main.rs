//! Crystal Forge Web UI — Dioxus web application.
//!
//! This is the entry point for the WASM web application.
//! It sets up routing, global state, and launches the Dioxus app.

mod api;
mod components;
mod dashboard;
mod environments;
mod routes;
mod state;
mod systems;
pub mod theme;
mod views;

use dioxus::prelude::*;

use routes::Route;
use state::app_state::{AuthFetchState, provide_app_state};

fn main() {
    dioxus::launch(app);
}

/// Check if UI check mock auth mode is enabled via query param.
/// Only available in debug builds to prevent production auth bypass.
#[cfg(debug_assertions)]
fn ui_check_mock_auth_enabled() -> bool {
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .map(|q| q.contains("ui_check_auth=1"))
        .unwrap_or(false)
}

#[cfg(not(debug_assertions))]
fn ui_check_mock_auth_enabled() -> bool {
    false
}

/// Root application component.
#[component]
fn app() -> Element {
    provide_app_state();

    // Fetch auth context on app initialization
    let mut app_state = use_context::<Signal<state::app_state::AppState>>();

    use_effect(move || {
        spawn(async move {
            // Skip API call if mock auth is enabled (for screenshot tests in debug builds)
            if ui_check_mock_auth_enabled() {
                let mut state = app_state.write();
                state.auth_fetch_state = AuthFetchState::Loaded;
                return;
            }
            match api::client::fetch_whoami().await {
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

    rsx! {
        // Load vendored Tailwind CSS (works offline).
        document::Stylesheet { href: asset!("assets/tailwind.min.css") }
        // App-specific theme variables and shared utility classes.
        document::Stylesheet { href: asset!("assets/app.css") }
        document::Link {
            rel: "icon",
            r#type: "image/png",
            href: asset!("assets/crystal-forge-icon.png")
        }
        document::Stylesheet {
            href: "https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/github-dark.min.css"
        }
        document::Script {
            src: "https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js",
        }
        Router::<Route> {}
    }
}
