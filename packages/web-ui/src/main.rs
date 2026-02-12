//! Crystal Forge Web UI — Dioxus web application.
//!
//! This is the entry point for the WASM web application.
//! It sets up routing, global state, and launches the Dioxus app.

mod api;
mod components;
mod routes;
mod state;
mod views;

use dioxus::prelude::*;

use routes::Route;
use state::app_state::provide_app_state;

fn main() {
    dioxus::launch(app);
}

/// Root application component.
fn app() -> Element {
    provide_app_state();

    rsx! {
        Router::<Route> {}
    }
}
