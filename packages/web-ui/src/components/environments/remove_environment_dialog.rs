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
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4",
            style: "position: fixed; inset: 0; z-index: 60; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
            onclick: move |_| props.on_cancel.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6",
                style: "width: 100%; max-width: 30rem;",
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
