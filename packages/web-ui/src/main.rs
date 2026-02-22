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

/// Check if UI check mock auth mode is enabled via query param.
fn ui_check_mock_auth_enabled() -> bool {
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .map(|q| q.contains("ui_check_auth=1"))
        .unwrap_or(false)
}

/// Root application component.
#[component]
fn app() -> Element {
    provide_app_state();

    // Fetch auth context on app initialization
    let mut app_state = use_context::<Signal<state::app_state::AppState>>();

    use_effect(move || {
        spawn(async move {
            // Skip API call if mock auth is enabled (for screenshot tests)
            if ui_check_mock_auth_enabled() {
                return;
            }
            if let Ok(auth_context) = api::client::fetch_whoami().await {
                app_state.write().auth = Some(auth_context);
            }
        });
    });

    rsx! {
        // Load vendored Tailwind CSS (works offline).
        document::Stylesheet { href: asset!("assets/tailwind.min.css") }
        document::Style {
            ":root {{ --cf-brand-purple: #82699B; --cf-brand-purple-hover: #B18FCC; --cf-danger-berry: #8A5E77; --cf-danger-berry-hover: #A3748E; }}\n.cf-primary-btn {{ background-color: var(--cf-brand-purple); }}\n.cf-primary-btn:hover {{ background-color: var(--cf-brand-purple-hover); }}\n.cf-danger-btn {{ background-color: var(--cf-danger-berry); }}\n.cf-danger-btn:hover {{ background-color: var(--cf-danger-berry-hover); }}\n.cf-builds-split {{ display: grid; grid-template-columns: minmax(0, 5fr) minmax(0, 7fr); gap: 1.5rem; }}\n.cf-flakes-history-split {{ display: grid; grid-template-columns: minmax(0, 4fr) minmax(0, 8fr); gap: 1.5rem; }}\n@media (max-width: 1280px) {{ .cf-flakes-history-split {{ grid-template-columns: minmax(0, 5fr) minmax(0, 7fr); }} }}\n@media (max-width: 1024px) {{ .cf-builds-split {{ grid-template-columns: 1fr; }} .cf-flakes-history-split {{ grid-template-columns: 1fr; }} }}"
        }
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
