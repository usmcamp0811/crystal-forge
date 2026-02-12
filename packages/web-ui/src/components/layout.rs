//! Application shell layout with sidebar navigation.

use dioxus::prelude::*;

use crate::routes::Route;

/// Top-level application layout wrapping all views.
///
/// Provides the sidebar navigation and main content area.
#[component]
pub fn AppLayout() -> Element {
    rsx! {
        div {
            class: "min-h-screen bg-gray-950 text-gray-100 flex",

            // Sidebar
            nav {
                class: "w-64 bg-gray-900 border-r border-gray-800 flex flex-col",
                div {
                    class: "p-6",
                    h1 {
                        class: "text-xl font-bold text-white",
                        "Crystal Forge"
                    }
                    p {
                        class: "text-xs text-gray-500 mt-1",
                        "Fleet Management"
                    }
                }
                div {
                    class: "flex-1 px-3 space-y-1",
                    NavLink { to: Route::DashboardView {}, label: "Dashboard", icon: "📊" }
                    NavLink { to: Route::SystemsView {}, label: "Systems", icon: "🖥️" }
                }
                div {
                    class: "p-4 border-t border-gray-800 text-xs text-gray-600",
                    "v0.1.0"
                }
            }

            // Main content
            main {
                class: "flex-1 overflow-auto",
                Outlet::<Route> {}
            }
        }
    }
}

/// A sidebar navigation link.
#[component]
fn NavLink(to: Route, label: &'static str, icon: &'static str) -> Element {
    // TODO: highlight active route
    rsx! {
        Link {
            to,
            class: "flex items-center gap-3 px-3 py-2 rounded-lg text-gray-400 hover:text-white hover:bg-gray-800 transition-colors",
            span { "{icon}" }
            span { "{label}" }
        }
    }
}
