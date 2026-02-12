//! Crystal Forge Web UI — Dioxus proof-of-concept.
//!
//! This is the entry point for the WASM web application.
//! It validates that Dioxus renders correctly in the browser
//! and establishes the foundational patterns for the full UI.

use dioxus::prelude::*;

fn main() {
    dioxus::launch(app);
}

/// Root application component.
fn app() -> Element {
    rsx! {
        div {
            class: "min-h-screen bg-gray-900 text-gray-100 flex flex-col items-center justify-center p-8",
            h1 {
                class: "text-4xl font-bold mb-2",
                "Crystal Forge"
            }
            p {
                class: "text-gray-400 mb-8",
                "Fleet Management Dashboard"
            }
            Counter {}
        }
    }
}

/// Simple counter component to validate Dioxus reactivity.
#[component]
fn Counter() -> Element {
    let mut count = use_signal(|| 0i32);

    rsx! {
        div {
            class: "bg-gray-800 rounded-lg p-6 shadow-lg",
            h2 {
                class: "text-lg font-semibold mb-4 text-gray-300",
                "Dioxus Reactivity Test"
            }
            div {
                class: "flex items-center gap-4",
                button {
                    class: "px-4 py-2 bg-red-600 hover:bg-red-700 rounded text-white font-medium transition-colors",
                    onclick: move |_| count -= 1,
                    "−"
                }
                span {
                    class: "text-3xl font-mono w-16 text-center",
                    "{count}"
                }
                button {
                    class: "px-4 py-2 bg-green-600 hover:bg-green-700 rounded text-white font-medium transition-colors",
                    onclick: move |_| count += 1,
                    "+"
                }
            }
            p {
                class: "text-sm text-gray-500 mt-4",
                "If the counter increments, Dioxus signals are working correctly."
            }
        }
    }
}
