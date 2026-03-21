//! Environments list view with add/remove and required policy assignment.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::components::environments::{
    environment_name_for_id, normalize_color_hex, normalize_optional, policy_library,
    required_agent_policy_id, validate_environment, validate_environment_edit, AddEnvironmentForm,
    EditEnvironmentDraft, EditEnvironmentModal, EditRequirementsModal, EnvironmentCard,
    EnvironmentItem, NewEnvironmentDraft, PolicyPickerModal, RemoveEnvironmentDialog,
};
use crate::components::notifications::{AlertBanner, AlertSeverity};
use crate::environments::adapter::{
    create_environment_via_api, delete_environment_via_api, load_environments_with_fallback,
    load_policies_with_fallback, update_environment_policies_via_api, update_environment_via_api,
};
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::theme;

fn came_from_setup() -> bool {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let flag = storage.get_item("cf.from_setup").ok().flatten();
        if flag.as_deref() == Some("1") {
            let _ = storage.remove_item("cf.from_setup");
            return true;
        }
    }
    false
}

#[component]
pub fn EnvironmentsListView() -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let state_read = app_state.read();
    let is_admin_user = auth::is_admin(&state_read.auth, &state_read.masquerade_role);

    let mut policy_library_state = use_signal(policy_library);
    let default_required_policy = required_agent_policy_id(&policy_library_state.read());

    // Shared config health (admin only) — used for contextual environment warnings.
    let config_health = app_state.read().config_health.clone();

    // Seed initial state from the backend API; fall back to deterministic mock
    // on error. The rest of the component's local-state CRUD (add/edit/remove)
    // continues to operate on the signal after initial load.
    let mut environments = use_signal(Vec::<EnvironmentItem>::new);
    let mut api_notice = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);
    let mut redirect_to_login = use_signal(|| false);

    let nav = use_navigator();

    use_effect(move || {
        spawn(async move {
            let policies_result = load_policies_with_fallback().await;
            let effective_default_policy = required_agent_policy_id(&policies_result.policies);
            let result = load_environments_with_fallback(effective_default_policy).await;

            if result.redirect_to_login || policies_result.redirect_to_login {
                redirect_to_login.set(true);
                loading.set(false);
                return;
            }

            let mut items = result.environments;
            items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

            environments.set(items);
            policy_library_state.set(policies_result.policies);

            let merged_notice = match (result.notice, policies_result.notice) {
                (Some(a), Some(b)) => Some(format!("{a}; {b}")),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            };

            api_notice.set(merged_notice);
            loading.set(false);
        });
    });

    if *redirect_to_login.read() {
        nav.push(Route::LoginView {});
        return rsx! {
            div {
                class: "flex items-center justify-center py-12",
                p { class: "{theme::text::SECONDARY}", "Redirecting to login..." }
            }
        };
    }

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
    let policy_library_for_add = policy_library_state.read().clone();

    let from_setup = use_signal(came_from_setup);
    let mut dismiss_add_target_callout = use_signal(|| false);

    rsx! {
        div {
            class: "space-y-6",

            if from_setup() {
                div {
                    "data-testid": "setup-coach-environments-callout",
                    style: "background:rgba(30,58,138,0.22); border:1px solid rgba(96,165,250,0.55); border-radius:8px; padding:12px 16px;",
                    p { style: "color:#dbeafe; font-size:12px; font-weight:700; margin:0; letter-spacing:0.03em; text-transform:uppercase;", "Setup Tour - Step 1 of 6" }
                    p { style: "color:#dbeafe; font-size:14px; font-weight:600; margin:4px 0 0 0;", "Create your first environment" }
                    p { style: "color:#bfdbfe; font-size:13px; margin:4px 0 0 0;", "Use Add Environment to define a deployment boundary like staging or production." }
                }
            }

            // API fallback notice banner
            if let Some(notice) = api_notice.read().clone() {
                div {
                    class: "flex items-center gap-2 px-4 py-3 rounded-lg border text-yellow-100 text-sm cf-chip-olive",
                    span { class: "shrink-0", "⚠" }
                    span { "{notice}" }
                }
            }

            // Admin-only contextual environment warnings from config health.
            if is_admin_user {
                if let Some(ref health) = config_health {
                    if !health.has_builders {
                        AlertBanner {
                            severity: AlertSeverity::Warning,
                            message: "No builder is registered. Builds for systems in any environment won't be processed.".to_string(),
                            action_label: Some("Add a builder".to_string()),
                            action_url: Some("/builders".to_string()),
                        }
                    }
                    if !health.has_cache_destinations {
                        AlertBanner {
                            severity: AlertSeverity::Warning,
                            message: "No cache destination is configured. Builds for environments won't be deployable.".to_string(),
                            action_label: Some("Add a cache".to_string()),
                            action_url: Some("/caches".to_string()),
                        }
                    }
                }
            }

            header {
                class: "flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Environment Registry" }
                    p { class: "text-sm {theme::text::SECONDARY}", "Group systems by deployment domain and define required deployment policy baselines." }
                }
                div {
                    class: "relative",
                    button {
                        class: if from_setup() && !*show_add_form.read() {
                            "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN} animate-pulse ring-2 ring-blue-300/70 ring-offset-2 ring-offset-slate-950"
                        } else {
                            "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}"
                        },
                        onclick: move |_| {
                            let next = !*show_add_form.read();
                            show_add_form.set(next);
                            add_error.set(None);
                            if next {
                                dismiss_add_target_callout.set(true);
                            }
                        },
                        if *show_add_form.read() { "Close" } else { "Add Environment" }
                    }
                    if from_setup() && !*show_add_form.read() && !dismiss_add_target_callout() {
                        div {
                            "data-testid": "setup-coach-environments-target-callout",
                            style: "position:absolute; right:0; top:calc(100% + 10px); background:rgba(30,64,175,0.94); border:1px solid rgba(96,165,250,0.75); border-radius:10px; padding:8px 10px; color:#dbeafe; font-size:12px; width:220px; box-shadow:0 10px 24px rgba(15,23,42,0.45);",
                            div {
                                style: "position:absolute; top:-6px; right:18px; width:10px; height:10px; background:rgba(30,64,175,0.94); border-left:1px solid rgba(96,165,250,0.75); border-top:1px solid rgba(96,165,250,0.75); transform:rotate(45deg);"
                            }
                            p { style: "margin:0; color:#eff6ff; font-weight:600;", "Next action" }
                            p { style: "margin:2px 0 0 0;", "Click Add Environment to create your first environment." }
                        }
                    }
                }
            }

            if *loading.read() {
                div {
                    class: "flex justify-center py-12",
                    div {
                        class: "flex flex-col items-center gap-3",
                        div {
                            class: "animate-spin rounded-full h-10 w-10 border-b-2 cf-spinner-accent",
                        }
                        p {
                            class: "text-sm {theme::text::SECONDARY}",
                            "Loading environments…"
                        }
                    }
                }
            }

            if !*loading.read() && *show_add_form.read() {
                AddEnvironmentForm {
                    draft: draft.clone(),
                    error: add_error.clone(),
                    policy_library: policy_library_state.read().clone(),
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

                        let mut environments = environments.clone();
                        let mut draft = draft.clone();
                        let mut add_error = add_error.clone();
                        let mut show_add_form = show_add_form.clone();
                        let mut api_notice = api_notice.clone();

                        spawn(async move {
                            match create_environment_via_api(
                                next.name.trim().to_string(),
                                normalize_optional(&next.description),
                                normalize_color_hex(&next.color_hex),
                                true,
                                default_required_policy,
                            )
                            .await
                            {
                                Ok(mut created) => {
                                    created.required_policy_ids = next.required_policy_ids;
                                    let mut values = environments.read().clone();
                                    values.push(created);
                                    values.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                                    environments.set(values);
                                    draft.set(NewEnvironmentDraft {
                                        name: String::new(),
                                        description: String::new(),
                                        color_hex: "#4F46E5".to_string(),
                                        required_policy_ids: vec![default_required_policy],
                                    });
                                    add_error.set(None);
                                    show_add_form.set(false);
                                }
                                Err(message) => {
                                    api_notice.set(Some(message));
                                }
                            }
                        });
                    },
                    on_choose_policies: move |_| show_add_policy_modal.set(true),
                }
            }

            if !*loading.read() && *show_add_policy_modal.read() {
                PolicyPickerModal {
                    title: "Choose Required Policies".to_string(),
                    current_ids: draft.read().required_policy_ids.clone(),
                    policy_library: policy_library_state.read().clone(),
                    on_apply: move |ids| {
                        let mut next = draft.read().clone();
                        next.required_policy_ids = ids;
                        draft.set(next);
                        show_add_policy_modal.set(false);
                    },
                    on_close: move |_| show_add_policy_modal.set(false),
                }
            }

            if !*loading.read() {
              div {
                class: "space-y-3",
                for env in items {
                    EnvironmentCard {
                        environment: env.clone(),
                        policy_library: policy_library_state.read().clone(),
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
            }

            if let Some(env_id) = editing_environment.read().clone() {
                EditRequirementsModal {
                    environment_name: environment_name_for_id(env_id, &environments.read()),
                    policy_library: policy_library_state.read().clone(),
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

                        let mut environments = environments.clone();
                        let env_id = env_id;
                        let mut editing_environment = editing_environment.clone();
                        let mut edit_error = edit_error.clone();
                        let mut api_notice = api_notice.clone();

                        spawn(async move {
                            // Call the API to update policies
                            match update_environment_policies_via_api(env_id, selected.clone()).await {
                                Ok(()) => {
                                    // Update local state on success
                                    let mut values = environments.read().clone();
                                    if let Some(target) = values.iter_mut().find(|env| env.id == env_id) {
                                        target.required_policy_ids = selected;
                                    }
                                    environments.set(values);
                                    editing_environment.set(None);
                                    edit_error.set(None);
                                }
                                Err(message) => {
                                    // Show error but still update local state for now
                                    api_notice.set(Some(message.clone()));
                                    edit_error.set(Some(message));
                                }
                            }
                        });
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

                        let mut environments = environments.clone();
                        let mut editing_environment_meta = editing_environment_meta.clone();
                        let mut edit_meta_error = edit_meta_error.clone();
                        let mut api_notice = api_notice.clone();

                        spawn(async move {
                            match update_environment_via_api(
                                next.id,
                                next.name.trim().to_string(),
                                normalize_optional(&next.description),
                                normalize_color_hex(&next.color_hex),
                                default_required_policy,
                            )
                            .await
                            {
                                Ok(updated) => {
                                    let mut values = environments.read().clone();
                                    if let Some(target) = values.iter_mut().find(|env| env.id == updated.id)
                                    {
                                        *target = updated;
                                    }
                                    values.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                                    environments.set(values);
                                    editing_environment_meta.set(None);
                                    edit_meta_error.set(None);
                                }
                                Err(message) => {
                                    api_notice.set(Some(message.clone()));
                                    edit_meta_error.set(Some(message));
                                }
                            }
                        });
                    },
                }
            }

            if let Some(env) = pending_remove.read().clone() {
                RemoveEnvironmentDialog {
                    environment: env,
                    on_cancel: move |_| pending_remove.set(None),
                    on_confirm: move |()| {
                        if let Some(ref env) = *pending_remove.read() {
                            let environment_id = env.id;
                            let mut environments = environments.clone();
                            let mut pending_remove = pending_remove.clone();
                            let mut api_notice = api_notice.clone();
                            spawn(async move {
                                match delete_environment_via_api(environment_id).await {
                                    Ok(()) => {
                                        let mut values = environments.read().clone();
                                        values.retain(|item| item.id != environment_id);
                                        environments.set(values);
                                        pending_remove.set(None);
                                    }
                                    Err(message) => {
                                        api_notice.set(Some(message));
                                        pending_remove.set(None);
                                    }
                                }
                            });
                        } else {
                            pending_remove.set(None);
                        }
                    },
                }
            }
        }
    }
}
