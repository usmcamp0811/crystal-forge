//! CVE view — placeholder for security posture.

use dioxus::prelude::*;

use crate::components::layout::Card;
use crate::theme;

/// The CVE dashboard page.
#[component]
pub fn CvesView() -> Element {
    rsx! {
        div {
            class: "space-y-6",
            h1 {
                class: "{theme::typography::PAGE_TITLE}",
                "CVE Dashboard"
            }
            Card {
                title: Some("Security Status".to_string()),
                children: rsx! {
                    p { class: "{theme::text::SECONDARY}", "Connect to API to view CVE data." }
                }
            }
        }
    }
}
