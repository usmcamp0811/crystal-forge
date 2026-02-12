//! Dashboard statistic card component.

use dioxus::prelude::*;

/// A card displaying a single numeric statistic with a label.
#[component]
pub fn StatCard(label: String, value: String, #[props(default)] color_class: String) -> Element {
    let text_color = if color_class.is_empty() {
        "text-white".to_string()
    } else {
        color_class
    };

    rsx! {
        div {
            class: "bg-gray-900 border border-gray-800 rounded-xl p-6",
            p {
                class: "text-sm text-gray-400 mb-1",
                "{label}"
            }
            p {
                class: "text-3xl font-bold {text_color}",
                "{value}"
            }
        }
    }
}
