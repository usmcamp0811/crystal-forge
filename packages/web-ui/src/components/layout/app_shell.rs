//! Application shell layout with sidebar navigation.

use dioxus::prelude::*;

use crate::components::layout::DevModeBanner;
use crate::components::layout::SidebarNav;
use crate::components::layout::TopBar;
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::theme;

/// Top-level application layout wrapping all views.
///
/// Provides the sidebar navigation and main content area.
/// Redirects to login if the user is not authenticated.
#[component]
pub fn AppShell() -> Element {
    let current_route = use_route::<Route>();
    let app_state = use_context::<Signal<AppState>>();
    let nav = navigator();

    // Check authentication and redirect if needed
    let auth_context = app_state.read().auth.clone();

    if !auth::is_authenticated(&auth_context) {
        // If auth context is loaded and user is not authenticated, redirect to login
        if auth_context.is_some() {
            nav.push("/login");
        }
        // Show loading while auth context is being fetched
        return rsx! {
            div {
                class: "min-h-screen flex items-center justify-center {theme::surface::PAGE_BG}",
                div {
                    class: "text-center",
                    div {
                        class: "animate-spin rounded-full h-12 w-12 border-b-2 border-violet-500 mx-auto mb-4"
                    }
                    p {
                        class: "{theme::text::SECONDARY}",
                        "Loading..."
                    }
                }
            }
        };
    }

    rsx! {
        div {
            class: "min-h-screen {theme::surface::PAGE_BG} {theme::text::PRIMARY} flex",

            SidebarNav {}

            div {
                class: "flex-1 flex flex-col min-w-0",
                TopBar { title: current_route.title() }
                DevModeBanner {}
                main {
                    class: "flex-1 overflow-auto {theme::spacing::PAGE_PADDING}",
                    Outlet::<Route> {}
                }
            }
        }
    }
}
