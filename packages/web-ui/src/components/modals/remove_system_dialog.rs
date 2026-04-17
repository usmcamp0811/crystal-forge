//! Remove system confirmation dialog.

use dioxus::prelude::*;

use crate::theme;

/// Confirmation dialog for disabling a system in the registry.
///
/// Disabling hides the system from active views while preserving history.
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
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
            onclick: move |_| on_cancel.call(()),

            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 cf-modal-panel-28",
                onclick: |evt| evt.stop_propagation(),

                h3 {
                    class: "text-lg font-semibold text-white text-center mb-2",
                    "Remove {hostname}?"
                }
                p {
                    class: "text-sm {theme::text::SECONDARY} text-center mb-6",
                    "This disables the system and hides it from active views. History is preserved."
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
                        "Disable"
                    }
                }
            }
        }
    }
}
