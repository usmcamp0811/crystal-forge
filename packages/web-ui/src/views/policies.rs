//! Policies view — global policy management for deployment rules.
//!
//! This view allows users to create, edit, and manage deployment policies
//! that can be applied to systems across the fleet.

use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use js_sys::Object;
use uuid::Uuid;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue};

use crate::components::layout::Card;
use crate::theme;

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

const POLICY_TOML_SAMPLE: &str = r#"[[policy]]
type = "require_crystal_forge_agent"
strict = true

[[policy]]
type = "require_packages"
packages = ["git", "vim"]
strict = false

[[policy]]
type = "custom_check"
expression = "(cfg.config.services.openssh.enable or false)"
description = "SSH must be enabled"
field_name = "sshEnabled"
strict = true
"#;

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyFormat {
    Toml,
    Json,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolicyDefinition {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub format: PolicyFormat,
    pub body: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PolicyPreset {
    RequireAgent,
    RequirePackages,
    CustomCheck,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
struct PolicyPresetMeta {
    id: Uuid,
    title: &'static str,
    description: &'static str,
    #[allow(dead_code)]
    summary: &'static str,
    #[allow(dead_code)]
    kind: PolicyPreset,
    format: PolicyFormat,
    body: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Main View
// ─────────────────────────────────────────────────────────────────────────────

/// The policies page for global policy management.
#[component]
pub fn PoliciesView() -> Element {
    let mut policy_library = use_signal(initial_policy_definitions);
    let mut show_editor = use_signal(|| false);
    let mut editing_policy_id: Signal<Option<Uuid>> = use_signal(|| None);
    let mut edit_name = use_signal(String::new);
    let mut edit_description = use_signal(String::new);
    let mut edit_body = use_signal(String::new);
    let mut edit_format = use_signal(|| PolicyFormat::Toml);
    let mut search_query = use_signal(String::new);
    let mut delete_confirm: Signal<Option<Uuid>> = use_signal(|| None);

    let query = search_query.read().to_lowercase();
    let filtered_policies: Vec<PolicyDefinition> = policy_library
        .read()
        .iter()
        .cloned()
        .filter(|policy| {
            if query.trim().is_empty() {
                return true;
            }
            policy.name.to_lowercase().contains(&query)
                || policy.description.to_lowercase().contains(&query)
        })
        .collect();

    let policy_count = policy_library.read().len();
    let filtered_count = filtered_policies.len();

    rsx! {
        div {
            class: "space-y-6",

            // Page header
            div {
                class: "flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Deployment Policies" }
                    p { class: "text-sm {theme::text::SECONDARY} mt-1",
                        "Manage deployment policies that can be applied to systems across your fleet."
                    }
                }
                button {
                    class: "inline-flex items-center gap-2 px-4 py-2 rounded-lg font-medium text-sm transition-all bg-violet-600 hover:bg-violet-500 text-white shadow-lg shadow-violet-900/30",
                    onclick: move |_| {
                        editing_policy_id.set(None);
                        edit_name.set(String::new());
                        edit_description.set(String::new());
                        edit_body.set(POLICY_TOML_SAMPLE.to_string());
                        edit_format.set(PolicyFormat::Toml);
                        show_editor.set(true);
                    },
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M12 4v16m8-8H4"
                        }
                    }
                    "New Policy"
                }
            }

            // Stats row
            div {
                class: "flex items-center gap-6 text-sm {theme::text::SECONDARY}",
                span {
                    class: "flex items-center gap-2",
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                        }
                    }
                    "{policy_count} total policies"
                }
                if !query.is_empty() {
                    span { class: "text-blue-400", "Showing {filtered_count} matching" }
                }
            }

            // Search and filter bar
            Card {
                children: rsx! {
                    div {
                        class: "flex items-center gap-3",
                        input {
                            class: "flex-1 rounded-lg border border-gray-700 bg-gray-900/70 px-4 py-2 text-sm text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500/40",
                            placeholder: "Search policies by name or description...",
                            value: "{search_query}",
                            oninput: move |event| search_query.set(event.value()),
                        }
                        div {
                            class: "w-10 h-10 rounded-lg border border-gray-700 bg-gray-900/70 flex items-center justify-center",
                            svg {
                                class: "w-4 h-4 text-gray-500",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M11 4a7 7 0 015.474 11.368l3.579 3.578-1.415 1.415-3.578-3.579A7 7 0 1111 4z"
                                }
                            }
                        }
                    }
                }
            }

            // Policy grid
            if filtered_policies.is_empty() {
                Card {
                    children: rsx! {
                        div {
                            class: "text-center py-12",
                            svg {
                                class: "w-12 h-12 mx-auto text-gray-600 mb-4",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "1.5",
                                    d: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                                }
                            }
                            if query.is_empty() {
                                p { class: "text-gray-400 mb-2", "No policies yet" }
                                p { class: "text-sm text-gray-500", "Create your first policy to get started." }
                            } else {
                                p { class: "text-gray-400 mb-2", "No policies match your search" }
                                p { class: "text-sm text-gray-500", "Try a different search term." }
                            }
                        }
                    }
                }
            } else {
                div {
                    class: "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4",
                    for policy in filtered_policies.iter().cloned() {
                        PolicyCard {
                            key: "{policy.id}",
                            policy: policy.clone(),
                            on_edit: move |p: PolicyDefinition| {
                                editing_policy_id.set(Some(p.id));
                                edit_name.set(p.name.clone());
                                edit_description.set(p.description.clone());
                                edit_body.set(p.body.clone());
                                edit_format.set(p.format);
                                show_editor.set(true);
                            },
                            on_delete: move |id: Uuid| {
                                delete_confirm.set(Some(id));
                            },
                        }
                    }
                }
            }

            // Editor modal
            if *show_editor.read() {
                PolicyEditorModal {
                    editing_policy_id: editing_policy_id.clone(),
                    edit_name: edit_name.clone(),
                    edit_description: edit_description.clone(),
                    edit_body: edit_body.clone(),
                    edit_format: edit_format.clone(),
                    policy_library: policy_library.clone(),
                    on_close: move || show_editor.set(false),
                }
            }

            // Delete confirmation modal
            if let Some(id) = *delete_confirm.read() {
                DeleteConfirmModal {
                    policy_id: id,
                    policy_name: policy_library.read().iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_default(),
                    on_confirm: move |_| {
                        let mut lib = policy_library.read().clone();
                        lib.retain(|p| p.id != id);
                        policy_library.set(lib);
                        delete_confirm.set(None);
                    },
                    on_cancel: move |_| {
                        delete_confirm.set(None);
                    },
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Policy Card Component
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn PolicyCard(
    policy: PolicyDefinition,
    on_edit: EventHandler<PolicyDefinition>,
    on_delete: EventHandler<Uuid>,
) -> Element {
    let mut expanded = use_signal(|| false);

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

// ─────────────────────────────────────────────────────────────────────────────
// Policy Editor Modal
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn PolicyEditorModal(
    editing_policy_id: Signal<Option<Uuid>>,
    edit_name: Signal<String>,
    edit_description: Signal<String>,
    edit_body: Signal<String>,
    edit_format: Signal<PolicyFormat>,
    policy_library: Signal<Vec<PolicyDefinition>>,
    on_close: EventHandler<()>,
) -> Element {
    let is_editing = editing_policy_id.read().is_some();
    let title = if is_editing {
        "Edit Policy"
    } else {
        "Create Policy"
    };
    let action_label = if is_editing {
        "Save Changes"
    } else {
        "Create Policy"
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-6",
            style: "position: fixed; inset: 0; z-index: 50; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
            onclick: move |_| on_close.call(()),

            div {
                class: "{theme::surface::CARD_BG} border border-violet-500/30 rounded-2xl p-6 shadow-xl shadow-violet-900/20",
                style: "width: 85vw; max-width: 64rem; display: flex; flex-direction: column; gap: 1.5rem;",
                onclick: |evt| evt.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between",
                    div {
                        class: "flex items-center gap-3",
                        div {
                            class: "w-10 h-10 rounded-lg bg-violet-500/20 flex items-center justify-center",
                            svg {
                                class: "w-5 h-5 text-violet-400",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                                }
                            }
                        }
                        div {
                            h3 { class: "text-white text-lg font-semibold", "{title}" }
                            p { class: "text-xs {theme::text::MUTED}", "Define the policy metadata and TOML/JSON body." }
                        }
                    }
                    button {
                        class: "p-2 rounded-lg text-gray-400 hover:text-white hover:bg-violet-500/10 transition-colors",
                        onclick: move |_| on_close.call(()),
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M6 18L18 6M6 6l12 12"
                            }
                        }
                    }
                }

                // Form content
                div {
                    class: "grid grid-cols-1 lg:grid-cols-[280px_1fr] gap-6 items-start",

                    // Left column - metadata
                    div {
                        class: "space-y-4",
                        div {
                            class: "space-y-2",
                            label { class: "text-xs text-violet-300/70 font-medium", "Policy Name" }
                            input {
                                class: "w-full rounded-lg border border-gray-700 bg-gray-950/50 px-3 py-2 text-sm text-gray-100 focus:outline-none focus:ring-2 focus:ring-violet-500/40 focus:border-violet-500/50",
                                placeholder: "e.g., Require SSH Enabled",
                                value: "{edit_name}",
                                oninput: move |event| edit_name.set(event.value()),
                            }
                        }
                        div {
                            class: "space-y-2",
                            label { class: "text-xs text-violet-300/70 font-medium", "Description" }
                            textarea {
                                class: "w-full rounded-lg border border-gray-700 bg-gray-950/50 px-3 py-2 text-sm text-gray-100 focus:outline-none focus:ring-2 focus:ring-violet-500/40 focus:border-violet-500/50 resize-none",
                                placeholder: "Describe what this policy enforces...",
                                rows: "4",
                                value: "{edit_description}",
                                oninput: move |event| edit_description.set(event.value()),
                            }
                        }
                        div {
                            class: "space-y-2",
                            label { class: "text-xs text-violet-300/70 font-medium", "Format" }
                            div {
                                class: "flex gap-2",
                                button {
                                    class: "px-3 py-1.5 rounded-md text-xs border transition-colors",
                                    class: if *edit_format.read() == PolicyFormat::Toml {
                                        "bg-violet-500/20 border-violet-500 text-violet-300"
                                    } else {
                                        "bg-gray-950/50 border-gray-700 text-gray-400 hover:border-gray-600"
                                    },
                                    onclick: move |_| edit_format.set(PolicyFormat::Toml),
                                    "TOML"
                                }
                                button {
                                    class: "px-3 py-1.5 rounded-md text-xs border transition-colors",
                                    class: if *edit_format.read() == PolicyFormat::Json {
                                        "bg-violet-500/20 border-violet-500 text-violet-300"
                                    } else {
                                        "bg-gray-950/50 border-gray-700 text-gray-400 hover:border-gray-600"
                                    },
                                    onclick: move |_| edit_format.set(PolicyFormat::Json),
                                    "JSON"
                                }
                            }
                        }
                    }

                    // Right column - code editor
                    div {
                        class: "space-y-3 flex flex-col",
                        label { class: "text-xs text-violet-300/70 font-medium", "Policy Definition" }
                        div {
                            class: "rounded-lg border border-gray-700 bg-gray-950/70 overflow-hidden",
                            textarea {
                                class: "w-full bg-transparent px-3 py-3 text-sm text-gray-100 font-mono focus:outline-none resize-none",
                                style: "min-height: 280px;",
                                rows: "12",
                                value: "{edit_body}",
                                oninput: move |event| edit_body.set(event.value()),
                                spellcheck: "false",
                            }
                        }
                    }
                }

                // Footer
                div {
                    class: "flex justify-end items-center gap-3 pt-4 border-t border-gray-800",
                    button {
                        class: "px-4 py-2 rounded-lg text-sm text-gray-300 border border-gray-700 hover:bg-gray-800 transition-colors",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 rounded-lg text-sm font-semibold bg-violet-600 hover:bg-violet-500 text-white transition-colors shadow-lg shadow-violet-900/30",
                        onclick: move |_| {
                            let name = edit_name.read().clone();
                            let description = edit_description.read().clone();
                            let body = edit_body.read().clone();
                            let format = *edit_format.read();
                            let new_id = editing_policy_id.read().unwrap_or_else(Uuid::new_v4);
                            let mut library = policy_library.read().clone();
                            let is_existing = library.iter().any(|policy| policy.id == new_id);

                            if is_existing {
                                library = library
                                    .into_iter()
                                    .map(|policy| {
                                        if policy.id == new_id {
                                            PolicyDefinition {
                                                id: new_id,
                                                name: name.clone(),
                                                description: description.clone(),
                                                format,
                                                body: body.clone(),
                                            }
                                        } else {
                                            policy
                                        }
                                    })
                                    .collect();
                            } else {
                                library.push(PolicyDefinition {
                                    id: new_id,
                                    name: name.clone(),
                                    description: description.clone(),
                                    format,
                                    body: body.clone(),
                                });
                            }
                            policy_library.set(library);
                            on_close.call(());
                        },
                        "{action_label}"
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Delete Confirmation Modal
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn DeleteConfirmModal(
    policy_id: Uuid,
    policy_name: String,
    on_confirm: EventHandler<()>,
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

                // Icon
                div {
                    class: "flex justify-center mb-4",
                    div {
                        class: "w-12 h-12 rounded-full bg-red-500/20 flex items-center justify-center",
                        svg {
                            class: "w-6 h-6 text-red-400",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                            }
                        }
                    }
                }

                // Title
                h3 {
                    class: "text-lg font-semibold text-white text-center mb-2",
                    "Delete Policy?"
                }

                // Description
                p {
                    class: "text-sm {theme::text::SECONDARY} text-center mb-6",
                    "Are you sure you want to delete "
                    span { class: "font-medium text-white", "{policy_name}" }
                    "? This action cannot be undone."
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
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-red-500 hover:bg-red-400 text-white",
                        onclick: move |_| on_confirm.call(()),
                        "Delete"
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

fn policy_presets() -> Vec<PolicyPresetMeta> {
    vec![
        PolicyPresetMeta {
            id: Uuid::from_u128(1),
            title: "Require Crystal Forge Agent",
            summary: "Agent services enabled",
            description: "This policy ensures the Crystal Forge agent and client services are enabled on the target system. It is a common baseline for production environments where you expect managed telemetry and deployments.",
            kind: PolicyPreset::RequireAgent,
            format: PolicyFormat::Toml,
            body: r#"[[policy]]
type = "require_crystal_forge_agent"
strict = true
"#
            .to_string(),
        },
        PolicyPresetMeta {
            id: Uuid::from_u128(2),
            title: "Require Packages",
            summary: "Package list guardrail",
            description: "Use this policy to guarantee specific system packages are present. It is useful for fleets where shared tooling (like git or vim) must be installed before deployments run.",
            kind: PolicyPreset::RequirePackages,
            format: PolicyFormat::Toml,
            body: r#"[[policy]]
type = "require_packages"
packages = ["git", "vim"]
strict = false
"#
            .to_string(),
        },
        PolicyPresetMeta {
            id: Uuid::from_u128(3),
            title: "Custom Check",
            summary: "Nix expression validation",
            description: "This policy lets you encode a custom Nix expression and description. It works well for environment-specific checks like enforcing SSH, ensuring a module is enabled, or validating configuration flags.",
            kind: PolicyPreset::CustomCheck,
            format: PolicyFormat::Toml,
            body: r#"[[policy]]
type = "custom_check"
expression = "(cfg.config.services.openssh.enable or false)"
description = "SSH must be enabled"
field_name = "sshEnabled"
strict = true
"#
            .to_string(),
        },
        PolicyPresetMeta {
            id: Uuid::from_u128(4),
            title: "Other Template",
            summary: "Flexible starter",
            description: "A flexible starting point for policies that do not fit the built-in templates. Use this when you want to annotate your own intent or create a specialized guardrail.",
            kind: PolicyPreset::Other,
            format: PolicyFormat::Toml,
            body: r#"[[policy]]
# Add your custom policy here
type = "custom_check"
expression = "(cfg.config.services.openssh.enable or false)"
description = "Describe requirement"
field_name = "customField"
strict = false
"#
            .to_string(),
        },
    ]
}

fn initial_policy_definitions() -> Vec<PolicyDefinition> {
    policy_presets()
        .into_iter()
        .map(|preset| PolicyDefinition {
            id: preset.id,
            name: preset.title.to_string(),
            description: preset.description.to_string(),
            format: preset.format,
            body: preset.body,
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Syntax Highlighting
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
