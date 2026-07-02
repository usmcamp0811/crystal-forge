//! Remove system confirmation dialog.

use dioxus::prelude::*;

use crate::components::icon::{Icon, IconName};
use crate::theme;

/// Confirmation dialog for disabling a system in the registry.
///
/// Disabling hides the system from active views while preserving history.
/// For production systems, requires typing the hostname to confirm.
#[component]
pub fn RemoveSystemDialog(
    /// The hostname of the system to remove
    hostname: String,
    /// The environment of the system (for production confirmation)
    environment: Option<String>,
    /// Whether the request is in progress
    is_loading: bool,
    /// Called when the user confirms removal
    on_confirm: EventHandler<()>,
    /// Called when the user cancels
    on_cancel: EventHandler<()>,
) -> Element {
    let mut confirm_text = use_signal(|| String::new());
    
    let is_production = environment
        .as_ref()
        .map(|e| e.eq_ignore_ascii_case("production"))
        .unwrap_or(false);
    
    let confirm_enabled = if is_production {
        !is_loading && confirm_text.read().trim() == hostname.trim()
    } else {
        !is_loading
    };

    rsx! {
        // Modal backdrop
        div {
            class: "modal-backdrop",
            onclick: move |_| {
                if !is_loading {
                    on_cancel.call(())
                }
            },

            // Modal panel
            div {
                class: "modal",
                onclick: |evt| evt.stop_propagation(),

                // Modal header
                div {
                    class: "modal-head",
                    div {
                        h2 { "Remove system from registry" }
                        div {
                            style: "font-size: 13px; color: var(--cf-text-secondary); margin-top: 2px;",
                            "{hostname}"
                        }
                    }
                    button {
                        class: "btn-icon focus-ring",
                        onclick: move |_| {
                            if !is_loading {
                                on_cancel.call(())
                            }
                        },
                        disabled: is_loading,
                        Icon { name: IconName::X, size: 16 }
                    }
                }

                // Modal body
                div {
                    class: "modal-body",

                    // Warning callout
                    div {
                        class: "sd-callout sd-callout-warn",
                        style: "margin-bottom: 16px;",
                        Icon { name: IconName::Warn, size: 13 }
                        span {
                            "Removing a system disables it and hides it from active views. History and audit logs are preserved but the system will no longer receive deployments."
                        }
                    }

                    if is_production {
                        div {
                            class: "field",
                            style: "margin-top: 16px;",
                            label { "Type the hostname to confirm removal from production" }
                            input {
                                r#type: "text",
                                placeholder: "{hostname}",
                                value: "{confirm_text}",
                                oninput: move |evt| confirm_text.set(evt.value()),
                                style: "width: 100%; padding: 8px 12px; border-radius: 6px; border: 1px solid var(--cf-card-border); font-family: var(--font-mono); font-size: 13px; box-sizing: border-box; background: var(--cf-card-bg); color: var(--cf-text-primary);",
                                disabled: is_loading,
                            }
                        }
                    }
                }

                // Modal footer
                div {
                    class: "modal-foot",
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| on_cancel.call(()),
                        disabled: is_loading,
                        "Cancel"
                    }
                    button {
                        class: "btn btn-danger focus-ring disabled:opacity-50 disabled:cursor-not-allowed",
                        onclick: move |_| {
                            if confirm_enabled {
                                on_confirm.call(())
                            }
                        },
                        disabled: !confirm_enabled,
                        if is_loading {
                            "Removing..."
                        } else {
                            "Remove system"
                        }
                    }
                }
            }
        }
    }
}
