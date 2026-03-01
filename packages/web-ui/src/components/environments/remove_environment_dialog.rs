//! Remove environment dialog component.

use dioxus::prelude::*;

use super::EnvironmentItem;
use crate::theme;

/// Props for the remove environment dialog.
#[derive(Props, Clone, PartialEq)]
pub struct RemoveEnvironmentDialogProps {
    pub environment: EnvironmentItem,
    pub on_cancel: EventHandler<()>,
    pub on_confirm: EventHandler<()>,
}

/// Confirmation dialog for removing an environment.
#[component]
pub fn RemoveEnvironmentDialog(props: RemoveEnvironmentDialogProps) -> Element {
    let env_name = props.environment.name.clone();

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
            onclick: move |_| props.on_cancel.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 cf-modal-panel-30",
                onclick: |evt| evt.stop_propagation(),
                h3 { class: "text-lg font-semibold text-white mb-2", "Remove environment {env_name}?" }
                p {
                    class: "text-sm {theme::text::SECONDARY} mb-6",
                    "This deletes the environment from the registry view."
                }
                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| props.on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-red-500 hover:bg-red-400 text-white",
                        onclick: move |_| props.on_confirm.call(()),
                        "Remove"
                    }
                }
            }
        }
    }
}
