//! Dashboard statistic card component.

use dioxus::prelude::*;

use crate::theme;

/// A card displaying a single numeric statistic with a label.
#[component]
pub fn StatCard(label: String, value: String, #[props(default)] color_class: String) -> Element {
    let text_color = if color_class.is_empty() {
        theme::text::PRIMARY.to_string()
    } else {
        color_class
    };

    rsx! {
        div {
            class: "{theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} rounded-xl p-6",
            p {
                class: "text-sm {theme::text::SECONDARY} mb-1",
                "{label}"
            }
            p {
                class: "text-3xl font-bold {text_color}",
                "{value}"
            }
        }
    }
}
