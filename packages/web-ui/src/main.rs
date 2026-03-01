//! Crystal Forge Web UI — Dioxus web application.
//!
//! This is the entry point for the WASM web application.
//! It sets up routing, global state, and launches the Dioxus app.

mod api;
mod bootstrap;
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
use state::app_state::provide_app_state;
use state::theme::{UiTheme, apply as apply_theme, persist as persist_theme};

fn main() {
    dioxus::launch(app);
}

/// Root application component.
#[component]
fn app() -> Element {
    provide_app_state();
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
