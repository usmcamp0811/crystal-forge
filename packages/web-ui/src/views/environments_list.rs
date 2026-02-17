//! Environments list view with add/remove and required policy assignment.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::components::layout::Card;
use crate::theme;

#[derive(Clone, Debug, PartialEq)]
struct PolicyOption {
    id: Uuid,
    name: String,
    description: String,
}

#[derive(Clone, Debug, PartialEq)]
struct EnvironmentItem {
    id: Uuid,
    name: String,
    description: Option<String>,
    system_count: usize,
    required_policy_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, PartialEq)]
struct NewEnvironmentDraft {
    name: String,
    description: String,
    required_policy_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, PartialEq)]
struct EditEnvironmentDraft {
    id: Uuid,
    name: String,
    description: String,
}

#[component]
pub fn EnvironmentsListView() -> Element {
    let policy_library = policy_library();
    let default_required_policy = required_agent_policy_id(&policy_library);

    let mut environments = use_signal(|| seed_environments(default_required_policy));
    let mut show_add_form = use_signal(|| false);
    let mut add_error = use_signal(|| None::<String>);
    let mut draft = use_signal(|| NewEnvironmentDraft {
        name: String::new(),
        description: String::new(),
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
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Environments" }
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
                Card {
                    title: Some("Create Environment".to_string()),
                    children: rsx! {
                        div {
                            class: "space-y-4",
                            div {
                                class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                                label {
                                    class: "space-y-2",
                                    span { class: "text-xs uppercase tracking-wide text-gray-500", "Name" }
                                    input {
                                        class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                        value: "{draft.read().name}",
                                        placeholder: "lan",
                                        oninput: move |evt| {
                                            let mut next = draft.read().clone();
                                            next.name = evt.value();
                                            draft.set(next);
                                        }
                                    }
                                }
                                label {
                                    class: "space-y-2",
                                    span { class: "text-xs uppercase tracking-wide text-gray-500", "Description (optional)" }
                                    input {
                                        class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                        value: "{draft.read().description}",
                                        placeholder: "Local area systems",
                                        oninput: move |evt| {
                                            let mut next = draft.read().clone();
                                            next.description = evt.value();
                                            draft.set(next);
                                        }
                                    }
                                }
                            }

                            div {
                                class: "rounded-lg border {theme::surface::CARD_BORDER} bg-gray-900/50 p-4 space-y-3",
                                div {
                                    class: "flex items-center justify-between gap-2",
                                    p { class: "text-xs uppercase tracking-wide text-gray-500", "Required Policies (all mandatory)" }
                                    button {
                                        class: "text-xs text-blue-300 hover:text-blue-200 px-2 py-1 rounded hover:bg-blue-500/10 transition-colors",
                                        onclick: move |_| show_add_policy_modal.set(true),
                                        "Choose Policies"
                                    }
                                }
                                div {
                                    class: "flex flex-wrap gap-2",
                                    for name in required_policy_names(&draft.read().required_policy_ids, &policy_library) {
                                        span {
                                            class: "inline-flex px-2 py-1 text-xs rounded border text-blue-100",
                                            style: "background-color: #253449; border-color: #3E5B82;",
                                            "{name}"
                                        }
                                    }
                                    if draft.read().required_policy_ids.is_empty() {
                                        span { class: "text-xs text-gray-500", "No required policies selected" }
                                    }
                                }
                            }

                            if let Some(message) = add_error.read().clone() {
                                p { class: "text-sm text-red-300", "{message}" }
                            }

                            div {
                                class: "flex flex-col-reverse sm:flex-row sm:justify-end gap-2",
                                button {
                                    class: "px-3 py-2 rounded-lg text-sm bg-gray-700 hover:bg-gray-600 text-white",
                                    onclick: move |_| {
                                        draft.set(NewEnvironmentDraft {
                                            name: String::new(),
                                            description: String::new(),
                                            required_policy_ids: vec![default_required_policy],
                                        });
                                        add_error.set(None);
                                        show_add_form.set(false);
                                    },
                                    "Cancel"
                                }
                                button {
                                    class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                                    onclick: move |_| {
                                        let next = draft.read().clone();
                                        if let Err(err) = validate_environment(&next, &environments.read(), &policy_library_for_add) {
                                            add_error.set(Some(err));
                                            return;
                                        }

                                        let mut values = environments.read().clone();
                                        values.push(EnvironmentItem {
                                            id: Uuid::new_v4(),
                                            name: next.name.trim().to_string(),
                                            description: normalize_optional(&next.description),
                                            system_count: 0,
                                            required_policy_ids: next.required_policy_ids,
                                        });
                                        values.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                                        environments.set(values);
                                        draft.set(NewEnvironmentDraft {
                                            name: String::new(),
                                            description: String::new(),
                                            required_policy_ids: vec![default_required_policy],
                                        });
                                        add_error.set(None);
                                        show_add_form.set(false);
                                    },
                                    "Save Environment"
                                }
                            }
                        }
                    }
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

            Card {
                title: Some("Environment Registry".to_string()),
                children: rsx! {
                    div {
                        class: "space-y-3",
                        for env in items {
                            {
                                let env_for_remove = env.clone();
                                let required_names = required_policy_names(&env.required_policy_ids, &policy_library);
                                let required_count = required_names.len();
                                let visible_chips: Vec<String> = required_names.iter().take(3).cloned().collect();
                                let overflow = required_count.saturating_sub(visible_chips.len());

                                rsx! {
                                    div {
                                        key: "{env.id}",
                                        class: "rounded-lg border {theme::surface::CARD_BORDER} bg-gray-900/60 p-4 space-y-3",
                                        div {
                                            class: "flex flex-col lg:flex-row lg:items-start lg:justify-between gap-3",
                                            div {
                                                p { class: "text-sm font-semibold text-white", "{env.name}" }
                                                p {
                                                    class: "text-xs {theme::text::SECONDARY}",
                                                    if let Some(description) = env.description.clone() {
                                                        "{description}"
                                                    } else {
                                                        "No description"
                                                    }
                                                }
                                            }
                                            div {
                                                class: "flex flex-wrap items-center gap-2 text-xs",
                                                span {
                                                    class: "inline-flex px-2 py-1 rounded border text-gray-100",
                                                    style: "background-color: #2B303B; border-color: #495264;",
                                                    "{env.system_count} systems"
                                                }
                                                span {
                                                    class: "inline-flex px-2 py-1 rounded border text-gray-100",
                                                    style: "background-color: #23363A; border-color: #3D6870;",
                                                    "{required_count} required"
                                                }
                                                span {
                                                    class: "inline-flex px-2 py-1 rounded border text-amber-100",
                                                    style: "background-color: #4A3B22; border-color: #8C6A2F;",
                                                    "Enforcement Pending"
                                                }
                                            }
                                        }

                                        div {
                                            class: "space-y-2",
                                            p { class: "text-xs uppercase tracking-wide text-gray-500", "Required Policies" }
                                            div {
                                                class: "flex flex-wrap gap-2",
                                                for policy_name in visible_chips {
                                                    span {
                                                        class: "inline-flex px-2 py-1 text-xs rounded border text-blue-100",
                                                        style: "background-color: #253449; border-color: #3E5B82;",
                                                        "{policy_name}"
                                                    }
                                                }
                                                if overflow > 0 {
                                                    span { class: "inline-flex px-2 py-1 text-xs rounded border border-gray-700 text-gray-400", "+{overflow}" }
                                                }
                                            }
                                        }

                                        div {
                                            class: "flex items-center justify-between pt-1",
                                            div {
                                                class: "flex items-center gap-2",
                                                button {
                                                    class: "text-xs text-blue-300 hover:text-blue-200 px-2 py-1 rounded hover:bg-blue-500/10 transition-colors",
                                                    onclick: {
                                                        let name = env.name.clone();
                                                        let description = env.description.clone().unwrap_or_default();
                                                        let id = env.id;
                                                        move |_| {
                                                            editing_environment_meta.set(Some(EditEnvironmentDraft {
                                                                id,
                                                                name: name.clone(),
                                                                description: description.clone(),
                                                            }));
                                                            edit_meta_error.set(None);
                                                        }
                                                    },
                                                    "Edit Environment"
                                                }
                                                button {
                                                    class: "text-xs text-blue-300 hover:text-blue-200 px-2 py-1 rounded hover:bg-blue-500/10 transition-colors",
                                                    onclick: {
                                                        let id = env.id;
                                                        let ids = env.required_policy_ids.clone();
                                                        move |_| {
                                                            editing_environment.set(Some(id));
                                                            editing_required_policy_ids.set(ids.clone());
                                                            edit_error.set(None);
                                                        }
                                                    },
                                                    "Edit Requirements"
                                                }
                                            }

                                            if env.system_count > 0 {
                                                span { class: "text-xs text-gray-500", "In Use" }
                                            } else {
                                                button {
                                                    class: "text-xs text-red-400 hover:text-red-300 px-2 py-1 rounded hover:bg-red-500/10 transition-colors",
                                                    onclick: move |_| pending_remove.set(Some(env_for_remove.clone())),
                                                    "Remove"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(env_id) = editing_environment.read().clone() {
                EditRequirementsModal {
                    environment_name: environment_name_for_id(env_id, &environments.read()),
                    policy_library: policy_library.clone(),
                    selected_policy_ids: editing_required_policy_ids,
                    error: edit_error,
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

            if let Some(editing) = editing_environment_meta.read().clone() {
                div {
                    class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4",
                    style: "position: fixed; inset: 0; z-index: 60; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
                    onclick: move |_| {
                        editing_environment_meta.set(None);
                        edit_meta_error.set(None);
                    },
                    div {
                        class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 space-y-4",
                        style: "width: 100%; max-width: 32rem;",
                        onclick: |evt| evt.stop_propagation(),

                        h3 { class: "text-lg font-semibold text-white", "Edit Environment" }
                        p { class: "text-sm {theme::text::SECONDARY}", "Update environment name and description." }

                        label {
                            class: "space-y-2 block",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Name" }
                            input {
                                class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{editing.name}",
                                oninput: move |evt| {
                                    let current = editing_environment_meta.read().clone();
                                    if let Some(mut next) = current {
                                        next.name = evt.value();
                                        editing_environment_meta.set(Some(next));
                                    }
                                }
                            }
                        }

                        label {
                            class: "space-y-2 block",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Description" }
                            input {
                                class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{editing.description}",
                                oninput: move |evt| {
                                    let current = editing_environment_meta.read().clone();
                                    if let Some(mut next) = current {
                                        next.description = evt.value();
                                        editing_environment_meta.set(Some(next));
                                    }
                                }
                            }
                        }

                        if let Some(message) = edit_meta_error.read().clone() {
                            p { class: "text-sm text-red-300", "{message}" }
                        }

                        div {
                            class: "flex flex-col-reverse sm:flex-row sm:justify-end gap-2",
                            button {
                                class: "px-3 py-2 rounded-lg text-sm bg-gray-700 hover:bg-gray-600 text-white",
                                onclick: move |_| {
                                    editing_environment_meta.set(None);
                                    edit_meta_error.set(None);
                                },
                                "Cancel"
                            }
                            button {
                                class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                                onclick: move |_| {
                                    let Some(next) = editing_environment_meta.read().clone() else {
                                        return;
                                    };
                                    if let Err(err) = validate_environment_edit(&next, &environments.read()) {
                                        edit_meta_error.set(Some(err));
                                        return;
                                    }

                                    let mut values = environments.read().clone();
                                    if let Some(target) = values.iter_mut().find(|env| env.id == next.id) {
                                        target.name = next.name.trim().to_string();
                                        target.description = normalize_optional(&next.description);
                                    }
                                    values.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                                    environments.set(values);
                                    editing_environment_meta.set(None);
                                    edit_meta_error.set(None);
                                },
                                "Save Changes"
                            }
                        }
                    }
                }
            }

            if let Some(env) = pending_remove.read().clone() {
                div {
                    class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4",
                    style: "position: fixed; inset: 0; z-index: 60; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
                    onclick: move |_| pending_remove.set(None),
                    div {
                        class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6",
                        style: "width: 100%; max-width: 30rem;",
                        onclick: |evt| evt.stop_propagation(),
                        h3 { class: "text-lg font-semibold text-white mb-2", "Remove environment {env.name}?" }
                        p {
                            class: "text-sm {theme::text::SECONDARY} mb-6",
                            "This deletes the environment from the registry view."
                        }
                        div {
                            class: "flex gap-3",
                            button {
                                class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white",
                                onclick: move |_| pending_remove.set(None),
                                "Cancel"
                            }
                            button {
                                class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-red-500 hover:bg-red-400 text-white",
                                onclick: move |_| {
                                    let mut values = environments.read().clone();
                                    values.retain(|item| item.id != env.id);
                                    environments.set(values);
                                    pending_remove.set(None);
                                },
                                "Remove"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EditRequirementsModal(
    environment_name: String,
    policy_library: Vec<PolicyOption>,
    selected_policy_ids: Signal<Vec<Uuid>>,
    error: Signal<Option<String>>,
    on_close: EventHandler<()>,
    on_save: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4",
            style: "position: fixed; inset: 0; z-index: 60; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
            onclick: move |_| on_close.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 space-y-4",
                style: "width: 100%; max-width: 44rem;",
                onclick: |evt| evt.stop_propagation(),

                    div {
                        class: "flex items-start justify-between gap-4",
                        div {
                            h3 { class: "text-lg font-semibold text-white", "Environment: {environment_name}" }
                            p { class: "text-sm {theme::text::SECONDARY}", "Required policies are hard requirements for this environment." }
                        }
                }

                div {
                    class: "rounded-lg border {theme::surface::CARD_BORDER} bg-gray-900/50 p-3 space-y-3 max-h-[50vh] overflow-y-auto",
                    for policy in policy_library {
                        {
                            let checked = selected_policy_ids.read().contains(&policy.id);
                            rsx! {
                                label {
                                    key: "{policy.id}",
                                    class: "rounded-lg border border-gray-700 px-3 py-2 flex items-start gap-3 cursor-pointer hover:bg-gray-800/60",
                                    style: if checked { "background-color: #1F2E42; border-color: #3E5B82;" } else { "" },
                                    input {
                                        r#type: "checkbox",
                                        checked,
                                        onchange: move |_| {
                                            let mut values = selected_policy_ids.read().clone();
                                            if values.contains(&policy.id) {
                                                values.retain(|id| id != &policy.id);
                                            } else {
                                                values.push(policy.id);
                                            }
                                            selected_policy_ids.set(values);
                                        }
                                    }
                                    div {
                                        p { class: "text-sm text-white", "{policy.name}" }
                                        p { class: "text-xs {theme::text::SECONDARY}", "{policy.description}" }
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(message) = error.read().clone() {
                    p { class: "text-sm text-red-300", "{message}" }
                }

                div {
                    class: "flex flex-col-reverse sm:flex-row sm:justify-end gap-2",
                    button {
                        class: "px-3 py-2 rounded-lg text-sm bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                        onclick: move |_| on_save.call(()),
                        "Save Requirements"
                    }
                }
            }
        }
    }
}

#[component]
fn PolicyPickerModal(
    title: String,
    current_ids: Vec<Uuid>,
    policy_library: Vec<PolicyOption>,
    on_apply: EventHandler<Vec<Uuid>>,
    on_close: EventHandler<()>,
) -> Element {
    let mut selected = use_signal(|| current_ids.clone());

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4",
            style: "position: fixed; inset: 0; z-index: 60; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
            onclick: move |_| on_close.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 space-y-4",
                style: "width: 100%; max-width: 44rem;",
                onclick: |evt| evt.stop_propagation(),

                h3 { class: "text-lg font-semibold text-white", "{title}" }
                p { class: "text-sm {theme::text::SECONDARY}", "Select policies that will be required for this environment." }

                div {
                    class: "rounded-lg border {theme::surface::CARD_BORDER} bg-gray-900/50 p-3 space-y-3 max-h-[50vh] overflow-y-auto",
                    for policy in policy_library {
                        {
                            let checked = selected.read().contains(&policy.id);
                            rsx! {
                                label {
                                    key: "{policy.id}",
                                    class: "rounded-lg border border-gray-700 px-3 py-2 flex items-start gap-3 cursor-pointer hover:bg-gray-800/60",
                                    style: if checked { "background-color: #1F2E42; border-color: #3E5B82;" } else { "" },
                                    input {
                                        r#type: "checkbox",
                                        checked,
                                        onchange: move |_| {
                                            let mut values = selected.read().clone();
                                            if values.contains(&policy.id) {
                                                values.retain(|id| id != &policy.id);
                                            } else {
                                                values.push(policy.id);
                                            }
                                            selected.set(values);
                                        }
                                    }
                                    div {
                                        p { class: "text-sm text-white", "{policy.name}" }
                                        p { class: "text-xs {theme::text::SECONDARY}", "{policy.description}" }
                                    }
                                }
                            }
                        }
                    }
                }

                div {
                    class: "flex flex-col-reverse sm:flex-row sm:justify-end gap-2",
                    button {
                        class: "px-3 py-2 rounded-lg text-sm bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                        onclick: move |_| on_apply.call(selected.read().clone()),
                        "Apply Policies"
                    }
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
    Ok(())
}

fn required_policy_names(ids: &[Uuid], policy_library: &[PolicyOption]) -> Vec<String> {
    ids.iter()
        .filter_map(|id| {
            policy_library
                .iter()
                .find(|policy| policy.id == *id)
                .map(|policy| policy.name.clone())
        })
        .collect()
}

fn required_agent_policy_id(policy_library: &[PolicyOption]) -> Uuid {
    policy_library
        .iter()
        .find(|policy| policy.name == "Require Crystal Forge Agent")
        .map(|policy| policy.id)
        .unwrap_or_else(|| Uuid::from_u128(1))
}

fn environment_name_for_id(id: Uuid, environments: &[EnvironmentItem]) -> String {
    environments
        .iter()
        .find(|env| env.id == id)
        .map(|env| env.name.clone())
        .unwrap_or_else(|| "Environment".to_string())
}

fn normalize_optional(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
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
            system_count: 12,
            required_policy_ids: vec![default_required_policy, Uuid::from_u128(3)],
        },
        EnvironmentItem {
            id: Uuid::from_u128(102),
            name: "staging".to_string(),
            description: Some("Pre-production validation".to_string()),
            system_count: 2,
            required_policy_ids: vec![default_required_policy],
        },
        EnvironmentItem {
            id: Uuid::from_u128(103),
            name: "development".to_string(),
            description: Some("Workstations and local testing".to_string()),
            system_count: 8,
            required_policy_ids: vec![default_required_policy],
        },
        EnvironmentItem {
            id: Uuid::from_u128(104),
            name: "remote".to_string(),
            description: Some("Remote unmanaged network".to_string()),
            system_count: 0,
            required_policy_ids: vec![default_required_policy],
        },
    ]
}
