//! Crystal Forge Web UI — Dioxus web application.
//!
//! This is the entry point for the WASM web application.
//! It sets up routing, global state, and launches the Dioxus app.

mod api;
mod components;
mod routes;
mod state;
pub mod theme;
mod views;

use dioxus::prelude::*;

use routes::Route;
use state::app_state::provide_app_state;

fn main() {
    dioxus::launch(app);
}

/// Root application component.
#[component]
fn app() -> Element {
    provide_app_state();

    rsx! {
        // Load vendored Tailwind CSS (works offline).
        document::Stylesheet { href: asset!("assets/tailwind.min.css") }
        document::Stylesheet {
            href: "https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/github-dark.min.css"
        }
        document::Script {
            src: "https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js",
            defer: "true"
        }
        Router::<Route> {}
    }
}
