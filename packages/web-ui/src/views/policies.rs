//! Policies view — global policy management for deployment rules.
//!
//! This view allows users to create, edit, and manage deployment policies
//! that can be applied to systems across the fleet.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::client::delete_deployment_policy;
use crate::components::layout::Card;
use crate::components::policy::{
    PolicyCard, PolicyDefinition, PolicyEditorModal, PolicyFormat, POLICY_TOML_SAMPLE,
};
use crate::theme;
use crate::views::policies_api;

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

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
    let mut policy_library: Signal<Vec<PolicyDefinition>> = use_signal(Vec::new);
    let mut show_editor = use_signal(|| false);
    
    // Load policies from API on mount
    use_effect(move || {
        spawn(async move {
            let policies = policies_api::load_policies_with_fallback().await;
            policy_library.set(policies);
        });
    });
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
                        let mut policy_library = policy_library;
                        let mut delete_confirm = delete_confirm;
                        spawn(async move {
                            match delete_deployment_policy(&id).await {
                                Ok(()) => {
                                    let latest = policies_api::load_policies_with_fallback().await;
                                    policy_library.set(latest);
                                }
                                Err(error) => {
                                    web_sys::console::error_1(
                                        &format!("Failed to delete policy: {error}").into(),
                                    );
                                }
                            }
                            delete_confirm.set(None);
                        });
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
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
            onclick: move |_| on_cancel.call(()),

            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 cf-modal-panel-28",
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
