//! Edit builder modal component.
//! TODO: Implement full form with environment assignment

use dioxus::prelude::*;
use uuid::Uuid;

#[component]
pub fn EditBuilderModal(
    builder_id: Uuid,
    on_close: EventHandler<()>,
    on_success: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-slate-800 border border-slate-700 rounded-lg p-6 max-w-2xl w-full mx-4",
                onclick: move |e| e.stop_propagation(),
                h2 {
                    class: "text-xl font-semibold text-white mb-4",
                    "Edit Builder"
                }
                p {
                    class: "text-slate-400 mb-2",
                    "Builder ID: {builder_id}"
                }
                p {
                    class: "text-slate-400 mb-4",
                    "Edit builder modal - implementation in progress"
                }
                div {
                    class: "flex justify-end gap-2",
                    button {
                        class: "px-4 py-2 text-slate-400 hover:text-white transition-colors",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                }
            }
        }
    }
}
