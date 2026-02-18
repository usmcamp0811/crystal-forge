//! Toast notification component.
//!
//! Displays temporary notification messages for user feedback.

use dioxus::prelude::*;

use crate::theme;

/// Toast notification for displaying success or error messages.
#[component]
pub fn Toast(message: String, is_success: bool, on_dismiss: EventHandler<()>) -> Element {
    let (bg_class, icon_class, icon_path) = if is_success {
        (
            "bg-emerald-900/90 border-emerald-700",
            "text-emerald-400",
            "M5 13l4 4L19 7", // checkmark
        )
    } else {
        (
            "bg-red-900/90 border-red-700",
            "text-red-400",
            "M6 18L18 6M6 6l12 12", // X
        )
    };

    rsx! {
        div {
            class: "animate-slide-in",
            style: "position: fixed; top: 1rem; right: 1rem; z-index: 120;",
            div {
                class: "flex items-center gap-3 px-4 py-3 rounded-lg border shadow-lg backdrop-blur-sm {bg_class}",

                // Icon
                div {
                    class: "shrink-0",
                    svg {
                        class: "w-5 h-5 {icon_class}",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "{icon_path}"
                        }
                    }
                }

                // Message
                span {
                    class: "text-sm text-white font-medium",
                    "{message}"
                }

                // Dismiss button
                button {
                    class: "shrink-0 ml-2 p-1 rounded hover:bg-white/10 transition-colors",
                    onclick: move |_| on_dismiss.call(()),
                    svg {
                        class: "w-4 h-4 text-gray-400",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M6 18L18 6M6 6l12 12"
                        }
                    }
                }
            }
        }
    }
}
