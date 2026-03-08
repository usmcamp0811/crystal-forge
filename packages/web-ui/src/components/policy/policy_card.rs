//! Policy card component for displaying policy definitions.

use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use js_sys::Object;
use uuid::Uuid;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue};

use crate::theme;

use super::types::{PolicyDefinition, PolicyFormat};

/// Card component for displaying a policy definition with expand/collapse and actions.
#[component]
pub fn PolicyCard(
    policy: PolicyDefinition,
    on_edit: EventHandler<PolicyDefinition>,
    on_delete: EventHandler<Uuid>,
) -> Element {
    let mut expanded = use_signal(|| false);
    let is_core_policy = policy.body.contains("type = \"require_cf_agent\"")
        || policy
            .body
            .contains("type = \"require_crystal_forge_agent\"");

    let format_badge = match policy.format {
        PolicyFormat::Toml => ("TOML", "bg-orange-500/20 text-orange-400"),
        PolicyFormat::Json => ("JSON", "bg-blue-500/20 text-blue-400"),
    };

    let policy_for_edit = policy.clone();
    let policy_id = policy.id;
    let line_count = policy.body.lines().count();
    let has_more = line_count > 4;

    // Get the language for syntax highlighting
    let language = match policy.format {
        PolicyFormat::Toml => "toml",
        PolicyFormat::Json => "json",
    };

    // Get preview or full content based on expanded state
    let display_text = if *expanded.read() {
        policy.body.clone()
    } else {
        policy.body.lines().take(4).collect::<Vec<_>>().join("\n")
    };

    let highlighted_html = highlight_code(language, &display_text);
    let chevron_class = if *expanded.read() { "rotate-180" } else { "" };

    rsx! {
        div {
            class: "group rounded-xl border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} p-4 hover:border-violet-500/40 transition-all",

            // Header
            div {
                class: "flex items-start justify-between gap-3 mb-3",
                div {
                    class: "flex-1 min-w-0",
                    h3 {
                        class: "text-sm font-semibold text-white truncate",
                        "{policy.name}"
                    }
                    p {
                        class: "text-xs text-gray-500 mt-1 line-clamp-2",
                        "{policy.description}"
                    }
                }
                span {
                    class: "shrink-0 text-xs font-medium px-2 py-0.5 rounded {format_badge.1}",
                    "{format_badge.0}"
                }
                if is_core_policy {
                    span {
                        class: "shrink-0 text-xs font-medium px-2 py-0.5 rounded bg-emerald-500/20 text-emerald-300 border border-emerald-400/40",
                        "Core · Always On"
                    }
                }
            }

            // Code preview with syntax highlighting
            div {
                class: "rounded-lg bg-gray-950/70 border border-gray-800 overflow-hidden mb-3",
                div {
                    class: "p-3 overflow-x-auto",
                    style: if *expanded.read() { "max-height: 400px; overflow-y: auto;" } else { "max-height: 100px; overflow: hidden;" },
                    pre {
                        class: "text-xs font-mono",
                        code {
                            class: "hljs language-{language}",
                            dangerous_inner_html: "{highlighted_html}"
                        }
                    }
                }

                // Expand/collapse button
                if has_more {
                    button {
                        class: "w-full flex items-center justify-center gap-1 py-1.5 text-xs text-violet-400 hover:text-violet-300 hover:bg-gray-800/50 border-t border-gray-800 transition-colors",
                        onclick: move |_| {
                            let current = *expanded.read();
                            expanded.set(!current);
                        },
                        if *expanded.read() {
                            "Show less"
                        } else {
                            "Show all {line_count} lines"
                        }
                        svg {
                            class: "w-3 h-3 transition-transform {chevron_class}",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M19 9l-7 7-7-7"
                            }
                        }
                    }
                }
            }

            // Actions
            div {
                class: "flex items-center justify-between pt-2 border-t border-gray-800",
                div {
                    class: "text-xs text-gray-500",
                    "{line_count} lines"
                }
                if is_core_policy {
                    div {
                        class: "text-xs text-emerald-300",
                        "Protected policy"
                    }
                } else {
                    div {
                        class: "flex items-center gap-2 opacity-0 group-hover:opacity-100 transition-opacity",
                        button {
                            class: "text-xs text-violet-400 hover:text-violet-300 px-2 py-1 rounded hover:bg-violet-500/10 transition-colors",
                            onclick: move |_| on_edit.call(policy_for_edit.clone()),
                            "Edit"
                        }
                        button {
                            class: "text-xs text-red-400 hover:text-red-300 px-2 py-1 rounded hover:bg-red-500/10 transition-colors",
                            onclick: move |_| on_delete.call(policy_id),
                            "Delete"
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Syntax Highlighting Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[cfg(target_arch = "wasm32")]
fn highlight_code(language: &str, text: &str) -> String {
    let Some(window) = web_sys::window() else {
        return escape_html(text);
    };
    let Ok(hljs) = js_sys::Reflect::get(&window, &JsValue::from_str("hljs")) else {
        return escape_html(text);
    };
    if hljs.is_undefined() || hljs.is_null() {
        return escape_html(text);
    }
    let Ok(highlight_fn) = js_sys::Reflect::get(&hljs, &JsValue::from_str("highlight")) else {
        return escape_html(text);
    };
    let Ok(highlight_fn) = highlight_fn.dyn_into::<js_sys::Function>() else {
        return escape_html(text);
    };
    let options = Object::new();
    let _ = js_sys::Reflect::set(
        &options,
        &JsValue::from_str("language"),
        &JsValue::from_str(language),
    );
    let Ok(result) = highlight_fn.call2(&hljs, &JsValue::from_str(text), &options.into()) else {
        return escape_html(text);
    };
    let Ok(value) = js_sys::Reflect::get(&result, &JsValue::from_str("value")) else {
        return escape_html(text);
    };
    value.as_string().unwrap_or_else(|| escape_html(text))
}

#[cfg(not(target_arch = "wasm32"))]
fn highlight_code(_language: &str, text: &str) -> String {
    escape_html(text)
}
