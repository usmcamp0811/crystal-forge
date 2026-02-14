//! Card container component with optional slots.

use dioxus::prelude::*;

use crate::theme;

/// Reusable card container with header/body/footer slots.
#[component]
pub fn Card(
    title: Option<String>,
    #[props(default)] header_actions: Option<Element>,
    children: Element,
    #[props(default)] footer: Option<Element>,
) -> Element {
    rsx! {
        div {
            class: "{theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} rounded-xl {theme::spacing::CARD_PADDING}",
            if let Some(title) = title {
                div {
                    class: "flex items-center justify-between mb-4",
                    h2 { class: "{theme::typography::SECTION_TITLE}", "{title}" }
                    if let Some(header_actions) = header_actions {
                        div { class: "text-sm {theme::text::SECONDARY}", {header_actions} }
                    }
                }
            }
            div { class: "space-y-3", {children} }
            if let Some(footer) = footer {
                div { class: "mt-4 pt-4 border-t {theme::surface::CARD_BORDER}", {footer} }
            }
        }
    }
}
