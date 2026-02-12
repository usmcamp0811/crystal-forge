//! Loading spinner and error display components.

use dioxus::prelude::*;

/// A centered loading spinner.
#[component]
pub fn LoadingSpinner() -> Element {
    rsx! {
        div {
            class: "flex items-center justify-center p-12",
            div {
                class: "animate-spin rounded-full h-8 w-8 border-b-2 border-blue-400",
            }
        }
    }
}

/// An error message display.
#[component]
pub fn ErrorMessage(message: String) -> Element {
    rsx! {
        div {
            class: "bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400",
            p {
                class: "font-medium",
                "Error"
            }
            p {
                class: "text-sm mt-1",
                "{message}"
            }
        }
    }
}
