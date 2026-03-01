//! Rollback confirmation dialog.
//!
//! Dialog for confirming system rollback to a historical commit.

use dioxus::prelude::*;

use crate::api::models::SystemCommitHistory;
use crate::theme;

/// Confirmation dialog for rolling back to a historical commit.
#[component]
pub fn RollbackConfirmDialog(
    hostname: String,
    commit: SystemCommitHistory,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let short_hash = commit.hash.chars().take(7).collect::<String>();

    rsx! {
        // Backdrop
        div {
            class: "bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
            onclick: move |_| on_cancel.call(()),

            // Dialog
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 cf-modal-panel-32",
                onclick: |evt| evt.stop_propagation(),

                // Icon
                div {
                    class: "flex justify-center mb-4",
                    div {
                        class: "w-12 h-12 rounded-full bg-amber-500/20 flex items-center justify-center",
                        svg {
                            class: "w-6 h-6 text-amber-400",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M3 12a9 9 0 1018 0 9 9 0 00-18 0zm9-4v4l-3 3"
                            }
                        }
                    }
                }

                // Title
                h3 {
                    class: "text-lg font-semibold text-white text-center mb-2",
                    "Deploy historical commit?"
                }

                // Description
                p {
                    class: "text-sm {theme::text::SECONDARY} text-center mb-4",
                    "This will roll back {hostname} to commit {short_hash}. This may pause automatic deployment policies."
                }

                // Commit summary
                div {
                    class: "rounded-lg border border-gray-700 bg-gray-900/60 p-3 mb-5",
                    div { class: "text-xs text-gray-400", "Commit" }
                    div { class: "text-sm text-white font-medium", "{commit.message}" }
                }

                // Buttons
                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-amber-500 hover:bg-amber-400 text-gray-900",
                        onclick: move |_| on_confirm.call(()),
                        "Deploy commit"
                    }
                }
            }
        }
    }
}
