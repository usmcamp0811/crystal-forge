//! Top bar layout component.

use dioxus::prelude::*;

use crate::theme;

/// Header bar displaying the current page title and optional actions.
#[component]
pub fn TopBar(title: String) -> Element {
    rsx! {
        header {
            class: "flex items-center justify-between h-16 px-6 border-b {theme::surface::CARD_BORDER} {theme::surface::SIDEBAR_BG}",
            div {
                class: "flex items-center gap-3",
                h1 {
                    class: "text-lg font-semibold",
                    "{title}"
                }
            }
            div {
                class: "hidden md:flex items-center gap-2",
                input {
                    class: "rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                    r#type: "search",
                    placeholder: "Search...",
                }
            }
        }
    }
}
