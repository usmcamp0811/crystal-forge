//! Environments list view with add/remove and required policy assignment.

use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use std::collections::HashMap;
use uuid::Uuid;

use crate::components::environments::{
    environment_name_for_id, normalize_color_hex, normalize_optional, required_agent_policy_id,
    required_policy_names, AddEnvironmentForm, EditEnvironmentDraft, EditEnvironmentModal,
    EditRequirementsModal, EnvironmentCard, EnvironmentItem, NewEnvironmentDraft, PolicyOption,
    PolicyPickerModal, RemoveEnvironmentDialog,
};
use crate::components::layout::Card;
use crate::theme;

const ENV_COLOR_STORAGE_KEY: &str = "crystal_forge.environments.colors";

#[component]
pub fn EnvironmentsListView() -> Element {
    let policy_library = policy_library();
    let default_required_policy = required_agent_policy_id(&policy_library);

    let mut environments = use_signal(|| initial_environments(default_required_policy));
    let mut show_add_form = use_signal(|| false);
    let mut add_error = use_signal(|| None::<String>);
    let mut draft = use_signal(|| NewEnvironmentDraft {
        name: String::new(),
        description: String::new(),
        color_hex: "#4F46E5".to_string(),
        required_policy_ids: vec![default_required_policy],
    });
    let mut pending_remove = use_signal(|| None::<EnvironmentItem>);
    let mut show_add_policy_modal = use_signal(|| false);
    let mut editing_environment_meta = use_signal(|| None::<EditEnvironmentDraft>);
    let mut edit_meta_error = use_signal(|| None::<String>);

    let mut editing_environment = use_signal(|| None::<Uuid>);
    let mut editing_required_policy_ids = use_signal(Vec::<Uuid>::new);
    let mut edit_error = use_signal(|| None::<String>);

    let items = environments.read().clone();
    let policy_library_for_add = policy_library.clone();

    rsx! {
        div {
            class: "space-y-6",
            header {
                class: "flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Environment Registry" }
                    p { class: "text-sm {theme::text::SECONDARY}", "Group systems by deployment domain and define required deployment policy baselines." }
                }
                button {
                    class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                    onclick: move |_| {
                        let next = !*show_add_form.read();
                        show_add_form.set(next);
                        add_error.set(None);
                    },
                    if *show_add_form.read() { "Close" } else { "Add Environment" }
                }
            }

            if *show_add_form.read() {
                AddEnvironmentForm {
                    draft: draft.clone(),
                    error: add_error.clone(),
                    policy_library: policy_library.clone(),
                    default_required_policy,
                    on_cancel: move |_| {
                        draft.set(NewEnvironmentDraft {
                            name: String::new(),
                            description: String::new(),
                            color_hex: "#4F46E5".to_string(),
                            required_policy_ids: vec![default_required_policy],
                        });
                        add_error.set(None);
                        show_add_form.set(false);
                    },
                    on_submit: move |next: NewEnvironmentDraft| {
                        if let Err(err) = validate_environment(&next, &environments.read(), &policy_library_for_add) {
                            add_error.set(Some(err));
                            return;
                        }

                        let mut values = environments.read().clone();
                        values.push(EnvironmentItem {
                            id: Uuid::new_v4(),
                            name: next.name.trim().to_string(),
                            description: normalize_optional(&next.description),
                            color_hex: normalize_color_hex(&next.color_hex),
                            system_count: 0,
                            required_policy_ids: next.required_policy_ids,
                        });
                        values.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                        persist_environment_colors(&values);
                        environments.set(values);
                        draft.set(NewEnvironmentDraft {
                            name: String::new(),
                            description: String::new(),
                            color_hex: "#4F46E5".to_string(),
                            required_policy_ids: vec![default_required_policy],
                        });
                        add_error.set(None);
                        show_add_form.set(false);
                    },
                    on_choose_policies: move |_| show_add_policy_modal.set(true),
                }
            }

            if *show_add_policy_modal.read() {
                PolicyPickerModal {
                    title: "Choose Required Policies".to_string(),
                    current_ids: draft.read().required_policy_ids.clone(),
                    policy_library: policy_library.clone(),
                    on_apply: move |ids| {
                        let mut next = draft.read().clone();
                        next.required_policy_ids = ids;
                        draft.set(next);
                        show_add_policy_modal.set(false);
                    },
                    on_close: move |_| show_add_policy_modal.set(false),
                }
            }

            div {
                class: "space-y-3",
                for env in items {
                    EnvironmentCard {
                        environment: env.clone(),
                        policy_library: policy_library.clone(),
                        on_edit_meta: move |e: EnvironmentItem| {
                            editing_environment_meta.set(Some(EditEnvironmentDraft {
                                id: e.id,
                                name: e.name,
                                description: e.description.unwrap_or_default(),
                                color_hex: e.color_hex,
                            }));
                            edit_meta_error.set(None);
                        },
                        on_edit_requirements: move |(id, ids): (Uuid, Vec<Uuid>)| {
                            editing_environment.set(Some(id));
                            editing_required_policy_ids.set(ids);
                            edit_error.set(None);
                        },
                        on_remove: move |e: EnvironmentItem| {
                            pending_remove.set(Some(e));
                        },
                    }
                }
            }

            if let Some(env_id) = editing_environment.read().clone() {
                EditRequirementsModal {
                    environment_name: environment_name_for_id(env_id, &environments.read()),
                    policy_library: policy_library.clone(),
                    selected_policy_ids: editing_required_policy_ids.clone(),
                    error: edit_error.clone(),
                    on_close: move |_| {
                        editing_environment.set(None);
                        edit_error.set(None);
                    },
                    on_save: move |_| {
                        let selected = editing_required_policy_ids.read().clone();
                        if selected.is_empty() {
                            edit_error.set(Some("At least one required policy must be selected.".to_string()));
                            return;
                        }

                        let mut values = environments.read().clone();
                        if let Some(target) = values.iter_mut().find(|env| env.id == env_id) {
                            target.required_policy_ids = selected;
                        }
                        environments.set(values);
                        editing_environment.set(None);
                        edit_error.set(None);
                    }
                }
            }

            if let Some(_) = editing_environment_meta.read().clone() {
                EditEnvironmentModal {
                    draft: editing_environment_meta.clone(),
                    error: edit_meta_error.clone(),
                    on_close: move |_| {
                        editing_environment_meta.set(None);
                        edit_meta_error.set(None);
                    },
                    on_save: move |next: EditEnvironmentDraft| {
                        if let Err(err) = validate_environment_edit(&next, &environments.read()) {
                            edit_meta_error.set(Some(err));
                            return;
                        }

                        let mut values = environments.read().clone();
                        if let Some(target) = values.iter_mut().find(|env| env.id == next.id) {
                            target.name = next.name.trim().to_string();
                            target.description = normalize_optional(&next.description);
                            target.color_hex = normalize_color_hex(&next.color_hex);
                        }
                        values.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                        persist_environment_colors(&values);
                        environments.set(values);
                        editing_environment_meta.set(None);
                        edit_meta_error.set(None);
                    },
                }
            }

            if let Some(env) = pending_remove.read().clone() {
                RemoveEnvironmentDialog {
                    environment: env,
                    on_cancel: move |_| pending_remove.set(None),
                    on_confirm: move |()| {
                        if let Some(ref env) = *pending_remove.read() {
                            let mut values = environments.read().clone();
                            values.retain(|item| item.id != env.id);
                            persist_environment_colors(&values);
                            environments.set(values);
                        }
                        pending_remove.set(None);
                    },
                }
            }
        }
    }
}

fn validate_environment(
    draft: &NewEnvironmentDraft,
    existing: &[EnvironmentItem],
    policy_library: &[PolicyOption],
) -> Result<(), String> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err("Environment name is required.".to_string());
    }
    if existing
        .iter()
        .any(|item| item.name.eq_ignore_ascii_case(name))
    {
        return Err("Environment already exists.".to_string());
    }
    if draft.required_policy_ids.is_empty() {
        return Err("At least one required policy must be selected.".to_string());
    }
    if !draft
        .required_policy_ids
        .iter()
        .all(|id| policy_library.iter().any(|policy| policy.id == *id))
    {
        return Err("Required policies must come from the policy library.".to_string());
    }
    if !looks_like_hex_color(&draft.color_hex) {
        return Err("Environment color must be a valid hex value.".to_string());
    }
    Ok(())
}

fn validate_environment_edit(
    draft: &EditEnvironmentDraft,
    existing: &[EnvironmentItem],
) -> Result<(), String> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err("Environment name is required.".to_string());
    }
    if existing
        .iter()
        .any(|item| item.id != draft.id && item.name.eq_ignore_ascii_case(name))
    {
        return Err("Environment name already exists.".to_string());
    }
    if !looks_like_hex_color(&draft.color_hex) {
        return Err("Environment color must be a valid hex value.".to_string());
    }
    Ok(())
}

fn looks_like_hex_color(value: &str) -> bool {
    if value.len() != 7 || !value.starts_with('#') {
        return false;
    }
    value[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

fn environment_color_map(items: &[EnvironmentItem]) -> HashMap<String, String> {
    items
        .iter()
        .map(|env| (env.name.to_lowercase(), normalize_color_hex(&env.color_hex)))
        .collect()
}

fn persist_environment_colors(items: &[EnvironmentItem]) {
    let map = environment_color_map(items);
    let _ = LocalStorage::set(ENV_COLOR_STORAGE_KEY, map);
}

fn load_environment_colors() -> HashMap<String, String> {
    LocalStorage::get::<HashMap<String, String>>(ENV_COLOR_STORAGE_KEY).unwrap_or_default()
}

fn initial_environments(default_required_policy: Uuid) -> Vec<EnvironmentItem> {
    let mut items = seed_environments(default_required_policy);
    let stored = load_environment_colors();
    for env in &mut items {
        if let Some(color) = stored.get(&env.name.to_lowercase()) {
            env.color_hex = normalize_color_hex(color);
        }
    }
    persist_environment_colors(&items);
    items
}

fn policy_library() -> Vec<PolicyOption> {
    vec![
        PolicyOption {
            id: Uuid::from_u128(1),
            name: "Require Crystal Forge Agent".to_string(),
            description: "Ensure Crystal Forge services are enabled on the target.".to_string(),
        },
        PolicyOption {
            id: Uuid::from_u128(2),
            name: "Require Packages".to_string(),
            description: "Guarantee required package set is installed.".to_string(),
        },
        PolicyOption {
            id: Uuid::from_u128(3),
            name: "Custom Check".to_string(),
            description: "Evaluate environment-specific Nix policy expression.".to_string(),
        },
    ]
}

fn seed_environments(default_required_policy: Uuid) -> Vec<EnvironmentItem> {
    vec![
        EnvironmentItem {
            id: Uuid::from_u128(101),
            name: "production".to_string(),
            description: Some("Live fleet systems".to_string()),
            color_hex: "#0F766E".to_string(),
            system_count: 12,
            required_policy_ids: vec![default_required_policy, Uuid::from_u128(3)],
        },
        EnvironmentItem {
            id: Uuid::from_u128(102),
            name: "staging".to_string(),
            description: Some("Pre-production validation".to_string()),
            color_hex: "#B45309".to_string(),
            system_count: 2,
            required_policy_ids: vec![default_required_policy],
        },
        EnvironmentItem {
            id: Uuid::from_u128(103),
            name: "development".to_string(),
            description: Some("Workstations and local testing".to_string()),
            color_hex: "#2563EB".to_string(),
            system_count: 8,
            required_policy_ids: vec![default_required_policy],
        },
        EnvironmentItem {
            id: Uuid::from_u128(104),
            name: "remote".to_string(),
            description: Some("Remote unmanaged network".to_string()),
            color_hex: "#6B7280".to_string(),
            system_count: 0,
            required_policy_ids: vec![default_required_policy],
        },
    ]
}
