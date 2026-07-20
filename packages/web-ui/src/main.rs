//! Crystal Forge Web UI — Dioxus web application.
//!
//! This is the entry point for the WASM web application.
//! It sets up routing, global state, and launches the Dioxus app.

mod alerts;
mod api;
mod bootstrap;
mod components;
mod dashboard;
mod environments;
mod export;
mod hooks;
mod routes;
mod showcase;
mod state;
mod systems;
pub mod theme;
mod utils;
mod views;

use dioxus::prelude::*;

use routes::Route;
use state::app_state::provide_app_state;
use state::navigation_focus::provide_navigation_focus;
use state::theme::{UiTheme, apply as apply_theme, persist as persist_theme};

fn main() {
    // Install a panic hook that logs Rust panic messages to the browser
    // developer console with full file/line info. Without this, WASM panics
    // produce only an opaque "RuntimeError: unreachable" trap with no message.
    console_error_panic_hook::set_once();

    dioxus::launch(app);
}

/// Root application component.
#[component]
fn app() -> Element {
    provide_app_state();
    provide_navigation_focus();
    let theme = use_context_provider(|| Signal::new(UiTheme::load()));

    use_effect(move || {
        let current = theme();
        apply_theme(current);
        persist_theme(current);
    });

    // Fetch auth context on app initialization
    let mut app_state = use_context::<Signal<state::app_state::AppState>>();

    // Initialize auth - this handles fetching user context
    bootstrap::auth::init_auth(app_state.clone());

    rsx! {
        // Inject CSS and JS assets
        {bootstrap::assets::inject_assets()}

        Router::<Route> {}
    }
}
