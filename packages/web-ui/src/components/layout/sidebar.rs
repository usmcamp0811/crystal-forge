//! Sidebar navigation component.

use dioxus::prelude::*;

use crate::routes::Route;
use crate::theme;

/// Sidebar navigation for primary routes.
#[component]
pub fn SidebarNav() -> Element {
    rsx! {
        nav {
            class: "hidden lg:flex w-64 {theme::surface::SIDEBAR_BG} border-r {theme::surface::CARD_BORDER} flex-col",
            div {
                class: "p-6",
                h1 {
                    class: "text-xl font-bold text-white",
                    "Crystal Forge"
                }
                p {
                    class: "text-xs {theme::text::MUTED} mt-1",
                    "Fleet Management"
                }
            }
            div {
                class: "flex-1 px-3 space-y-1",
                NavLink { to: Route::DashboardView {}, label: "Dashboard", icon: "📊" }
                NavLink { to: Route::SystemsView {}, label: "Systems", icon: "🖥️" }
                NavLink { to: Route::BuildsView {}, label: "Builds", icon: "🧱" }
                NavLink { to: Route::CvesView {}, label: "CVEs", icon: "🛡️" }
                NavLink { to: Route::StyleGuideView {}, label: "Style Guide", icon: "🎨" }
            }
            div {
                class: "p-4 border-t {theme::surface::CARD_BORDER} text-xs {theme::text::MUTED}",
                "v0.1.0"
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
