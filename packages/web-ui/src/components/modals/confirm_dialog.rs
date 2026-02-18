//! Generic confirmation dialog component.

use dioxus::prelude::*;

use crate::theme;

#[derive(Props, Clone, PartialEq)]
pub struct ConfirmDialogProps {
    /// Title displayed at the top of the dialog
    pub title: String,
    /// Description text shown below the title
    pub description: String,
    /// Label for the confirm button (e.g., "Delete", "Confirm")
    #[props(default = "Confirm".to_string())]
    pub confirm_label: String,
    /// Whether to show a danger variant for the confirm button
    #[props(default = false)]
    pub danger: bool,
    /// Callback when user confirms
    pub on_confirm: EventHandler<()>,
    /// Callback when user cancels
    pub on_cancel: EventHandler<()>,
}

/// A generic confirmation dialog with cancel and confirm buttons.
///
/// Renders as a modal overlay with a title, description, and two buttons.
/// Clicking outside the dialog or pressing cancel dismisses it.
///
/// # Example
/// ```ignore
/// let mut show_dialog = use_signal(|| false);
///
/// rsx! {
///     if *show_dialog.read() {
///         ConfirmDialog {
///             title: "Delete item?".to_string(),
///             description: "This action cannot be undone.".to_string(),
///             confirm_label: "Delete".to_string(),
///             danger: true,
///             on_confirm: move |_| {
///                 // Handle confirm
///                 show_dialog.set(false);
///             },
///             on_cancel: move |_| show_dialog.set(false),
///         }
///     }
/// }
/// ```
#[component]
pub fn ConfirmDialog(props: ConfirmDialogProps) -> Element {
    let confirm_btn_class = if props.danger {
        "bg-red-500 hover:bg-red-400 text-white"
    } else {
        &theme::interactive::PRIMARY_BTN
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4",
            style: "position: fixed; inset: 0; z-index: 60; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
            onclick: move |_| props.on_cancel.call(()),

            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6",
                style: "width: 100%; max-width: 30rem;",
                onclick: |evt| evt.stop_propagation(),

                h3 {
                    class: "text-lg font-semibold text-white mb-2",
                    "{props.title}"
                }
                p {
                    class: "text-sm {theme::text::SECONDARY} mb-6",
                    "{props.description}"
                }

                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| props.on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors {confirm_btn_class}",
                        onclick: move |_| props.on_confirm.call(()),
                        "{props.confirm_label}"
                    }
                }
            }
        }
    }
}
