//! Builds view — placeholder for build pipeline visibility.

use dioxus::prelude::*;

use crate::components::layout::Card;
use crate::theme;

/// The builds page.
#[component]
pub fn BuildsView() -> Element {
    rsx! {
        div {
            class: "space-y-6",
            h1 {
                class: "{theme::typography::PAGE_TITLE}",
                "Builds"
            }
            Card {
                title: Some("Pipeline Activity".to_string()),
                children: rsx! {
                    p { class: "{theme::text::SECONDARY}", "Connect to API to view build history." }
                }
            }
        }
    }
}
