//! 404 Not Found page.

use dioxus::prelude::*;

/// Displayed when no route matches the current URL.
#[component]
pub fn NotFoundView(route: Vec<String>) -> Element {
    let path = route.join("/");
    rsx! {
        div {
            class: "p-8 flex flex-col items-center justify-center min-h-[60vh]",
            h1 {
                class: "text-6xl font-bold text-gray-700 mb-4",
                "404"
            }
            p {
                class: "text-gray-400 mb-6",
                "Page not found: /{path}"
            }
            Link {
                to: crate::routes::Route::DashboardView {},
                class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded-lg text-white transition-colors",
                "Go to Dashboard"
            }
        }
    }
}
