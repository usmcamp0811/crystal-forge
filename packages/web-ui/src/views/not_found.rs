//! 404 Not Found page.

use dioxus::prelude::*;

use crate::theme;

/// Displayed when no route matches the current URL.
#[component]
pub fn NotFoundView(route: Vec<String>) -> Element {
    let path = route.join("/");
    rsx! {
        div {
            class: "flex flex-col items-center justify-center min-h-[60vh]",
            h1 {
                class: "text-6xl font-bold text-gray-700 mb-4",
                "404"
            }
            p {
                class: "{theme::text::SECONDARY} mb-6",
                "Page not found: /{path}"
            }
            Link {
                to: crate::routes::Route::DashboardView {},
                class: "px-4 py-2 rounded-lg text-white transition-colors {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING}",
                "Go to Dashboard"
            }
        }
    }
}
