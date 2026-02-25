//! Environments list view with add/remove and required policy assignment.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::components::environments::{
    AddEnvironmentForm, EditEnvironmentDraft, EditEnvironmentModal, EditRequirementsModal,
    EnvironmentCard, EnvironmentItem, NewEnvironmentDraft, PolicyOption, PolicyPickerModal,
    RemoveEnvironmentDialog, environment_name_for_id, normalize_color_hex, normalize_optional,
    policy_library, required_agent_policy_id, validate_environment, validate_environment_edit,
};
use crate::environments::adapter::{
    create_environment_via_api, delete_environment_via_api, load_environments_with_fallback,
    update_environment_policies_via_api, update_environment_via_api,
};
use crate::routes::Route;
use crate::theme;

#[component]
pub fn EnvironmentsListView() -> Element {
    let policy_library = policy_library();
    let default_required_policy = required_agent_policy_id(&policy_library);

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
            let result = load_environments_with_fallback(default_required_policy).await;

            if result.redirect_to_login {
                redirect_to_login.set(true);
                loading.set(false);
                return;
            }

            let mut items = result.environments;
            items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

            environments.set(items);
            api_notice.set(result.notice);
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
    let policy_library_for_add = policy_library.clone();

    rsx! {
        div {
            class: "space-y-6",

            // API fallback notice banner
            if let Some(notice) = api_notice.read().clone() {
                div {
                    class: "flex items-center gap-2 px-4 py-3 rounded-lg border text-yellow-100 text-sm",
                    style: "background-color: #3B2F00; border-color: #7A6000;",
                    span { class: "shrink-0", "⚠" }
                    span { "{notice}" }
                }
            }

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

            if *loading.read() {
                div {
                    class: "flex justify-center py-12",
                    div {
                        class: "flex flex-col items-center gap-3",
                        div {
                            class: "animate-spin rounded-full h-10 w-10 border-b-2",
                            style: "border-color: #82699B;",
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

            if !*loading.read() {
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
