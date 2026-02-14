//! System detail view — full information for a single NixOS system.

use dioxus::prelude::*;

use crate::components::layout::Card;
use crate::theme;

/// The system detail page, reached via `/systems/:id`.
#[component]
pub fn SystemDetailView(id: String) -> Element {
    // TODO: Replace with real API call using use_resource + fetch_system()
    rsx! {
        div {
            class: "space-y-6",
            div {
                Link {
                    to: crate::routes::Route::SystemsView {},
                    class: "text-sm {theme::text::SECONDARY} hover:text-white transition-colors",
                    "← Back to Systems"
                }
            }

            h1 {
                class: "{theme::typography::PAGE_TITLE}",
                "System Detail: {id}"
            }

            div {
                class: "grid grid-cols-1 lg:grid-cols-3 gap-6",
                Card {
                    title: Some("Status".to_string()),
                    children: rsx! {
                        p { class: "{theme::text::SECONDARY}", "Connect to API to view system status." }
                    }
                }

                Card {
                    title: Some("Hardware".to_string()),
                    children: rsx! {
                        p { class: "{theme::text::SECONDARY}", "Connect to API to view hardware info." }
                    }
                }

                Card {
                    title: Some("Security".to_string()),
                    children: rsx! {
                        p { class: "{theme::text::SECONDARY}", "Connect to API to view security posture." }
                    }
                }
            }
        }
    }
}
