//! Remove system confirmation dialog.

use dioxus::prelude::*;

use crate::theme;

/// Confirmation dialog for removing a system from the registry.
///
/// Shows a warning that the action only removes from the current view.
#[component]
pub fn RemoveSystemDialog(
    /// The hostname of the system to remove
    hostname: String,
    /// Called when the user confirms removal
    on_confirm: EventHandler<()>,
    /// Called when the user cancels
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4",
            style: "position: fixed; inset: 0; z-index: 60; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
            onclick: move |_| on_cancel.call(()),

            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6",
                style: "width: 100%; max-width: 28rem;",
                onclick: |evt| evt.stop_propagation(),

                h3 {
                    class: "text-lg font-semibold text-white text-center mb-2",
                    "Remove {hostname}?"
                }
                p {
                    class: "text-sm {theme::text::SECONDARY} text-center mb-6",
                    "This removes the system from the current registry view."
                }
                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-red-500 hover:bg-red-400 text-white",
                        onclick: move |_| on_confirm.call(()),
                        "Remove"
                    }
                }
            }
        }
    }
}
