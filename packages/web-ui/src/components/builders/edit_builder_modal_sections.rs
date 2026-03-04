use dioxus::prelude::*;

use crate::theme;

#[component]
pub fn KeyRotationSection(
    is_submitting: bool,
    mut rotated_public_key: Signal<String>,
    rotated_private_key: String,
    show_rotated_private_key: bool,
    on_generate_keypair: EventHandler<MouseEvent>,
    on_toggle_private_key: EventHandler<MouseEvent>,
    on_apply_public_key: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div {
            class: "border border-slate-700 rounded p-4 space-y-3",
            div {
                class: "flex items-center justify-between",
                h3 {
                    class: "text-sm font-medium {theme::text::PRIMARY}",
                    "Rotate Builder Keypair"
                }
                button {
                    class: "px-3 py-1 rounded-lg text-sm font-medium text-white transition-colors {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING}",
                    onclick: move |e| on_generate_keypair.call(e),
                    disabled: is_submitting,
                    "🔑 Generate New Keypair"
                }
            }
            p {
                class: "text-xs {theme::text::SECONDARY}",
                "Paste a public key manually or generate a new keypair. Apply to save the public key to this builder."
            }

            div {
                label {
                    class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                    "Public Key (base64)"
                }
                textarea {
                    class: "w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded text-white font-mono text-xs",
                    rows: "2",
                    value: "{rotated_public_key()}",
                    oninput: move |e| rotated_public_key.set(e.value()),
                    disabled: is_submitting,
                }
            }

            if !rotated_private_key.is_empty() {
                div {
                    div {
                        class: "flex items-center justify-between mb-1",
                        label {
                            class: "block text-sm font-medium text-amber-400",
                            "Generated Private Key"
                            span { class: "text-xs {theme::text::SECONDARY} ml-2", "(save this securely)" }
                        }
                        button {
                            class: "text-xs text-blue-400 hover:text-blue-300",
                            onclick: move |e| on_toggle_private_key.call(e),
                            if show_rotated_private_key { "Hide" } else { "Show" }
                        }
                    }
                    if show_rotated_private_key {
                        textarea {
                            class: "w-full px-3 py-2 bg-amber-900/20 border border-amber-700/50 rounded text-amber-200 font-mono text-xs",
                            rows: "2",
                            readonly: true,
                            value: "{rotated_private_key}",
                        }
                    } else {
                        div {
                            class: "px-3 py-2 bg-slate-900 border border-slate-700 rounded text-slate-500 font-mono text-xs",
                            "••••••••••••••••••••••••••••••••"
                        }
                    }
                }
            }

            div {
                class: "pt-1",
                button {
                    class: "px-4 py-2 rounded-lg text-sm font-medium text-white transition-colors {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING} disabled:opacity-50",
                    onclick: move |e| on_apply_public_key.call(e),
                    disabled: is_submitting || rotated_public_key().trim().is_empty(),
                    if is_submitting {
                        "Applying..."
                    } else {
                        "Apply Public Key Update"
                    }
                }
            }
        }
    }
}
