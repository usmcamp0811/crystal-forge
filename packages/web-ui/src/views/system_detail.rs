//! System detail view — full information for a single NixOS system.

use dioxus::prelude::*;

/// The system detail page, reached via `/systems/:id`.
#[component]
pub fn SystemDetailView(id: String) -> Element {
    // TODO: Replace with real API call using use_resource + fetch_system()
    rsx! {
        div {
            class: "p-8",
            // Back link
            div {
                class: "mb-6",
                Link {
                    to: crate::routes::Route::SystemsView {},
                    class: "text-sm text-gray-400 hover:text-white transition-colors",
                    "← Back to Systems"
                }
            }

            h1 {
                class: "text-2xl font-bold mb-6",
                "System Detail: {id}"
            }

            // Info cards placeholder
            div {
                class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                // Status card
                div {
                    class: "bg-gray-900 border border-gray-800 rounded-xl p-6",
                    h2 { class: "text-lg font-semibold mb-4", "Status" }
                    p { class: "text-gray-500 text-sm", "Connect to API to view system status." }
                }

                // Hardware card
                div {
                    class: "bg-gray-900 border border-gray-800 rounded-xl p-6",
                    h2 { class: "text-lg font-semibold mb-4", "Hardware" }
                    p { class: "text-gray-500 text-sm", "Connect to API to view hardware info." }
                }

                // Security card
                div {
                    class: "bg-gray-900 border border-gray-800 rounded-xl p-6",
                    h2 { class: "text-lg font-semibold mb-4", "Security" }
                    p { class: "text-gray-500 text-sm", "Connect to API to view security posture." }
                }
            }
        }
    }
}
