//! Application shell layout with sidebar navigation.

use dioxus::prelude::*;

use crate::components::layout::DevModeBanner;
use crate::components::layout::SidebarNav;
use crate::components::layout::TopBar;
use crate::routes::Route;
use crate::theme;

/// Top-level application layout wrapping all views.
///
/// Provides the sidebar navigation and main content area.
#[component]
pub fn AppShell() -> Element {
    let current_route = use_route::<Route>();

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
