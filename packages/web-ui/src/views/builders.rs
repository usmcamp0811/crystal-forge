//! Builders management view.

use dioxus::prelude::*;

use crate::components::builders::BuildersList;
use crate::theme;

/// Builders management page.
#[component]
pub fn BuildersView() -> Element {
    rsx! {
        div {
            class: "space-y-6",

            header {
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Builders" }
                    p {
                        class: "text-sm {theme::text::SECONDARY}",
                        "Manage build workers and their environment assignments."
                    }
                }
            }

            BuildersList {}
        }
    }
}
