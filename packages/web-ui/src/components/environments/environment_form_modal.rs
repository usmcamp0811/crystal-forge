//! Unified Add/Edit environment modal for the design-parity Environments view.

use dioxus::prelude::*;

use super::{
    EnvironmentDeploymentPolicy, EnvironmentFormDraft, EnvironmentItem, PolicyOption,
    looks_like_hex_color,
};
use crate::components::icon::{Icon, IconName};

#[derive(Props, Clone, PartialEq)]
pub struct EnvironmentFormModalProps {
    pub draft: Signal<Option<EnvironmentFormDraft>>,
    pub existing: Vec<EnvironmentItem>,
    pub policy_library: Vec<PolicyOption>,
    pub error: Signal<Option<String>>,
    pub on_close: EventHandler<()>,
    pub on_save: EventHandler<EnvironmentFormDraft>,
    pub on_remove: EventHandler<EnvironmentItem>,
}

pub fn validate_environment_form(
    draft: &EnvironmentFormDraft,
    existing: &[EnvironmentItem],
) -> Result<(), String> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err("Environment name is required.".to_string());
    }
    if existing
        .iter()
        .any(|item| Some(item.id) != draft.id && item.name.eq_ignore_ascii_case(name))
    {
        return Err("Environment name already exists.".to_string());
    }
    if !looks_like_hex_color(&draft.color_hex) {
        return Err("Environment color must be a valid hex value.".to_string());
    }
    Ok(())
}

#[component]
pub fn EnvironmentFormModal(props: EnvironmentFormModalProps) -> Element {
    let mut draft = props.draft;
    let error = props.error;
    let Some(current) = draft.read().clone() else {
        return rsx! {};
    };
    let is_edit = current.id.is_some();
    let matching_env = current
        .id
        .and_then(|id| props.existing.iter().find(|env| env.id == id).cloned());
    let matching_system_plural = matching_env
        .as_ref()
        .map(|env| if env.system_count == 1 { "" } else { "s" })
        .unwrap_or("");

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| props.on_close.call(()),
            div { class: "modal", style: "width:min(620px,96vw); max-height:92vh;", onclick: |evt| evt.stop_propagation(),
                div { class: "modal-head",
                    h2 {
                        Icon { name: if is_edit { IconName::Gear } else { IconName::Plus }, size: 14 }
                        " "
                        if is_edit { "Edit {current.name}" } else { "Add environment" }
                    }
                    p { if is_edit { "Update environment settings, cache assignment, and deployment policy." } else { "Create a new environment tier." } }
                }

                div { class: "modal-body", style: "overflow-y:auto; display:flex; flex-direction:column; gap:14px;",
                    div { style: "display:grid; grid-template-columns:1fr 1fr; gap:14px;",
                        div { class: "field",
                            label { "Name" }
                            input {
                                class: "input focus-ring mono",
                                value: "{current.name}",
                                placeholder: "e.g. production",
                                oninput: move |evt| update_draft(&mut draft, |next| next.name = evt.value())
                            }
                        }
                        ColorPicker { draft }
                    }

                    div { class: "field",
                        label { "Description" }
                        input {
                            class: "input focus-ring",
                            value: "{current.description}",
                            placeholder: "What this tier is for",
                            oninput: move |evt| update_draft(&mut draft, |next| next.description = evt.value())
                        }
                    }

                    CacheSection { environment_name: current.name.clone() }
                    DeploymentPolicySection { draft }
                    PolicyEnforcementSection { draft, policy_library: props.policy_library.clone() }
                    ProductionToggle { draft }

                    div { style: "display:flex; gap:18px; flex-wrap:wrap;",
                        label { style: "display:flex; gap:8px; align-items:center; font-size:13px; cursor:pointer;",
                            input {
                                r#type: "checkbox",
                                checked: current.auto_sync.unwrap_or(true),
                                onchange: move |evt| update_draft(&mut draft, |next| next.auto_sync = Some(evt.checked())),
                                style: "accent-color:var(--cf-brand-purple);",
                            }
                            span { "Auto-sync flakes" }
                        }
                        label { style: "display:flex; gap:8px; align-items:center; font-size:13px; cursor:pointer;",
                            input {
                                r#type: "checkbox",
                                checked: current.requires_approval.unwrap_or(true),
                                onchange: move |evt| update_draft(&mut draft, |next| next.requires_approval = Some(evt.checked())),
                                style: "accent-color:var(--cf-brand-purple);",
                            }
                            span { "Require approval before deploy" }
                        }
                    }

                    if is_edit {
                        div { style: "margin-top:10px; padding-top:14px; border-top:1px solid var(--cf-divider);",
                            div { style: "font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:0.08em; color:var(--cf-text-muted); margin-bottom:8px;", "Danger zone" }
                            if let Some(env) = matching_env.clone() {
                                button {
                                    class: "btn btn-ghost focus-ring",
                                    style: "color:#f87171; border-color:rgba(248,113,113,0.3);",
                                    onclick: move |_| props.on_remove.call(env.clone()),
                                    Icon { name: IconName::X, size: 12 }
                                    " Remove environment"
                                }
                                if env.system_count > 0 {
                                    div { class: "help", style: "margin-top:6px;",
                                        Icon { name: IconName::Warn, size: 10 }
                                        " {env.system_count} system{matching_system_plural} currently use this env. Reassign them first."
                                    }
                                }
                            }
                        }
                    }

                    if let Some(message) = error.read().clone() {
                        div { class: "sd-callout sd-callout-danger", Icon { name: IconName::Warn, size: 13 } div { "{message}" } }
                    }
                }

                div { class: "modal-foot",
                    button { class: "btn btn-ghost focus-ring", onclick: move |_| props.on_close.call(()), "Cancel" }
                    button { class: "btn btn-primary focus-ring", onclick: move |_| props.on_save.call(current.clone()), Icon { name: IconName::Check, size: 13 } if is_edit { " Save changes" } else { " Add environment" } }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ColorPickerProps {
    draft: Signal<Option<EnvironmentFormDraft>>,
}

#[component]
fn ColorPicker(props: ColorPickerProps) -> Element {
    let mut draft = props.draft;
    let Some(current) = draft.read().clone() else {
        return rsx! {};
    };
    let colors = [
        ("red", "#dc2626"),
        ("amber", "#d97706"),
        ("emerald", "#059669"),
        ("blue", "#2563eb"),
        ("teal", "#0f766e"),
        ("violet", "#7c3aed"),
        ("pink", "#db2777"),
        ("slate", "#475569"),
    ];
    rsx! {
        div { class: "field",
            label { "Color" }
            div { style: "display:flex; gap:6px; flex-wrap:wrap; align-items:center;",
                for (name, value) in colors {
                    {
                        let border = if current.color_hex.eq_ignore_ascii_case(value) {
                            "2px solid var(--cf-text-primary)"
                        } else {
                            "2px solid transparent"
                        };
                        rsx! {
                    button {
                        class: "focus-ring",
                        title: "{name}",
                        style: "width:28px; height:28px; border-radius:8px; cursor:pointer; background:{value}; border:{border};",
                        onclick: move |_| update_draft(&mut draft, |next| next.color_hex = value.to_string())
                    }
                        }
                    }
                }
                label { class: "focus-ring", title: "Custom color", style: "width:28px; height:28px; border-radius:8px; cursor:pointer; background:{current.color_hex}; border:2px solid var(--cf-card-border); display:flex; align-items:center; justify-content:center; color:white;",
                    Icon { name: IconName::Plus, size: 12 }
                    input {
                        r#type: "color",
                        value: "{current.color_hex}",
                        style: "opacity:0; position:absolute; width:0; height:0;",
                        oninput: move |evt| update_draft(&mut draft, |next| next.color_hex = evt.value())
                    }
                }
                span { class: "mono", style: "font-size:11px; color:var(--cf-text-muted); margin-left:4px;", "{current.color_hex}" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct CacheSectionProps {
    environment_name: String,
}

#[component]
fn CacheSection(props: CacheSectionProps) -> Element {
    rsx! {
        div { style: "padding:14px; border:1px solid var(--cf-divider); border-radius:10px; background:color-mix(in oklab,var(--cf-page-bg) 50%,var(--cf-card-bg));",
            div { style: "display:flex; align-items:center; justify-content:space-between; gap:6px; margin-bottom:10px;",
                div { style: "font-size:13px; font-weight:600; display:flex; align-items:center; gap:6px;", Icon { name: IconName::Download, size: 13 } " Binary cache" }
                span { style: "font-size:11px; color:var(--cf-text-muted);", "Temporary placeholder — TASK-360" }
            }
            select { class: "input focus-ring", disabled: true,
                option { "No cache assigned" }
                option { selected: true, "Placeholder cache for {props.environment_name}" }
            }
            div { class: "help", "Cache assignment persistence is tracked by TASK-360." }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct DeploymentPolicySectionProps {
    draft: Signal<Option<EnvironmentFormDraft>>,
}

#[component]
fn DeploymentPolicySection(props: DeploymentPolicySectionProps) -> Element {
    let mut draft = props.draft;
    let Some(current) = draft.read().clone() else {
        return rsx! {};
    };
    let policies = [
        (EnvironmentDeploymentPolicy::Manual, "Manual"),
        (EnvironmentDeploymentPolicy::AutoLatest, "Auto latest"),
        (EnvironmentDeploymentPolicy::Pinned, "Pinned"),
    ];
    rsx! {
        div { class: "field",
            label { "Default deployment mode" }
            div { class: "seg", style: "width:fit-content; flex-wrap:wrap;",
                for (policy, label) in policies {
                    button {
                        class: if current.default_policy == Some(policy) { "active" } else { "" },
                        disabled: true,
                        title: "TASK-359 tracks persisted environment deployment policy",
                        "{label}"
                    }
                }
            }
            div { class: "help", "Display-only until TASK-359 persists environment deployment policy." }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PolicyEnforcementSectionProps {
    draft: Signal<Option<EnvironmentFormDraft>>,
    policy_library: Vec<PolicyOption>,
}

#[component]
fn PolicyEnforcementSection(props: PolicyEnforcementSectionProps) -> Element {
    let mut draft = props.draft;
    let Some(current) = draft.read().clone() else {
        return rsx! {};
    };
    rsx! {
        div { style: "padding:14px; border:1px solid var(--cf-divider); border-radius:10px; background:color-mix(in oklab,var(--cf-page-bg) 50%,var(--cf-card-bg));",
            div { style: "display:flex; align-items:center; justify-content:space-between; gap:6px; margin-bottom:4px;",
                div { style: "font-size:13px; font-weight:600; display:flex; align-items:center; gap:6px;", Icon { name: IconName::Shield, size: 13 } " Policy enforcement" }
                span { style: "font-size:11px; color:var(--cf-text-muted);", "Applied to every system in this env" }
            }
            div { class: "help", style: "margin-top:0; margin-bottom:12px;", "Baseline policy picker uses existing environment policies. Gate policies and compliance bundles are tracked by TASK-361." }
            div { style: "font-size:11px; font-weight:600; color:var(--cf-text-secondary); margin-bottom:6px;", "Gate policies" }
            div { style: "display:flex; flex-wrap:wrap; gap:6px; margin-bottom:8px;",
                for policy in props.policy_library.clone() {
                    {
                        let policy_id = policy.id;
                        let selected = current.required_policy_ids.contains(&policy_id);
                        rsx! {
                            button {
                                class: if selected { "chip chip-info focus-ring" } else { "chip chip-unknown focus-ring" },
                                style: "cursor:pointer;",
                                title: "{policy.description}",
                                onclick: move |_| update_draft(&mut draft, |next| {
                                    if next.required_policy_ids.contains(&policy_id) {
                                        next.required_policy_ids.retain(|id| *id != policy_id);
                                    } else {
                                        next.required_policy_ids.push(policy_id);
                                    }
                                }),
                                "{policy.name}"
                            }
                        }
                    }
                }
            }
            div { style: "font-size:11px; font-weight:600; color:var(--cf-text-secondary); margin-bottom:6px; display:flex; justify-content:space-between;",
                span { "Required compliance bundle" }
                span { style: "font-size:10px; color:var(--cf-text-muted); font-weight:400;", "placeholder" }
            }
            select { class: "input focus-ring", disabled: true,
                option { "None — no compliance bundle required" }
                option { "DISA STIG (placeholder)" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ProductionToggleProps {
    draft: Signal<Option<EnvironmentFormDraft>>,
}

#[component]
fn ProductionToggle(props: ProductionToggleProps) -> Element {
    let mut draft = props.draft;
    let Some(current) = draft.read().clone() else {
        return rsx! {};
    };
    let border = if current.is_production.unwrap_or(false) {
        "color-mix(in oklab, var(--cf-danger-berry) 55%, var(--cf-card-border))"
    } else {
        "var(--cf-card-border)"
    };
    let background = if current.is_production.unwrap_or(false) {
        "color-mix(in oklab, var(--cf-danger-berry) 10%, transparent)"
    } else {
        "transparent"
    };
    rsx! {
        label { class: "env-prod-toggle", style: "display:flex; gap:11px; align-items:flex-start; cursor:pointer; padding:11px 13px; border:1px solid {border}; border-radius:10px; background:{background}; margin-bottom:14px;",
            input {
                r#type: "checkbox",
                checked: current.is_production.unwrap_or(false),
                onchange: move |evt| update_draft(&mut draft, |next| next.is_production = Some(evt.checked())),
                style: "accent-color:var(--cf-danger-berry); margin-top:2px;",
            }
            span { style: "min-width:0;",
                span { style: "display:flex; align-items:center; gap:7px; font-size:13px; font-weight:600;",
                    Icon { name: IconName::Shield, size: 13 }
                    "Production environment"
                }
                span { style: "display:block; font-size:11.5px; color:var(--cf-text-muted); margin-top:3px; line-height:1.45;",
                    "Marks this environment as production. Affects how the UI highlights the environment and future policy enforcement."
                }
            }
        }
    }
}

fn update_draft(
    draft: &mut Signal<Option<EnvironmentFormDraft>>,
    update: impl FnOnce(&mut EnvironmentFormDraft),
) {
    let current = draft.read().clone();
    if let Some(mut next) = current {
        update(&mut next);
        draft.set(Some(next));
    }
}
