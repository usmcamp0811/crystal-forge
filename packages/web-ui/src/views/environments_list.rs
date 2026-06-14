//! Environments list view with CrystalForgelatest parity.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::components::environments::{
    environment_name_for_id, normalize_color_hex, normalize_optional, policy_library,
    required_agent_policy_id, validate_environment, validate_environment_form, EnvironmentCard,
    EnvironmentDeploymentPolicy, EnvironmentFormDraft, EnvironmentFormModal, EnvironmentItem,
    EnvironmentTable, NewEnvironmentDraft, PolicyOption, RemoveEnvironmentDialog,
};
use crate::components::icon::{Icon, IconName};
use crate::components::notifications::{AlertBanner, AlertSeverity};
use crate::environments::adapter::{
    create_environment_via_api, delete_environment_via_api, load_environments_with_fallback,
    load_policies_with_fallback, update_environment_policies_via_api, update_environment_via_api,
};
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewMode {
    Cards,
    Table,
}

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
    let is_admin_user = auth::is_admin(&app_state.read().auth);
    let config_health = app_state.read().config_health.clone();

    let mut policy_library_state = use_signal(policy_library);
    let default_required_policy = required_agent_policy_id(&policy_library_state.read());

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

            api_notice.set(match (result.notice, policies_result.notice) {
                (Some(a), Some(b)) => Some(format!("{a}; {b}")),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            });
            loading.set(false);
        });
    });

    if *redirect_to_login.read() {
        nav.push(Route::LoginView {});
        return rsx! {
            div { class: "flex items-center justify-center py-12",
                p { class: "{theme::text::SECONDARY}", "Redirecting to login..." }
            }
        };
    }

    let from_setup = use_signal(came_from_setup);
    let mut query = use_signal(String::new);
    let mut view_mode = use_signal(|| ViewMode::Cards);
    let mut form_draft = use_signal(|| None::<EnvironmentFormDraft>);
    let mut form_error = use_signal(|| None::<String>);
    let mut pending_remove = use_signal(|| None::<EnvironmentItem>);

    let items = environments.read().clone();
    let filtered = filtered_environments(&items, &query());
    let totals = EnvironmentTotals::from(&items);

    rsx! {
        div { style: "display:flex; flex-direction:column; gap:16px;",
            if from_setup() {
                div { "data-testid": "setup-coach-environments-callout", class: "sd-callout sd-callout-info",
                    Icon { name: IconName::Plus, size: 13 }
                    div {
                        div { style: "font-size:12px; font-weight:700; text-transform:uppercase; letter-spacing:0.03em;", "Setup Tour - Step 1 of 6" }
                        div { style: "font-size:13px;", "Create your first environment using Add environment." }
                    }
                }
            }

            if let Some(notice) = api_notice.read().clone() {
                div { class: "flex items-center gap-2 px-4 py-3 rounded-lg border text-yellow-100 text-sm cf-chip-olive",
                    span { class: "shrink-0", "⚠" }
                    span { "{notice}" }
                }
            }

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

            div { class: "page-head",
                div {
                    h1 { class: "page-title", "Environments" }
                    p { class: "page-subtitle", "{items.len()} tiers · {totals.systems} systems · {totals.caches}/{items.len()} caches configured" }
                }
                div { style: "display:flex; gap:8px;",
                    button {
                        class: "btn btn-primary focus-ring",
                        "data-coach-target": "env",
                        onclick: move |_| {
                            form_error.set(None);
                            form_draft.set(Some(new_environment_form_draft(default_required_policy)));
                        },
                        Icon { name: IconName::Plus, size: 14 }
                        " Add environment"
                    }
                }
            }

            StatStrip { totals: totals.clone(), total_tiers: items.len() }

            div { class: "filterbar",
                div { class: "filter-search", style: "max-width:320px;",
                    Icon { name: IconName::Search }
                    input {
                        class: "input focus-ring",
                        placeholder: "Search environments…",
                        value: "{query}",
                        oninput: move |evt| query.set(evt.value())
                    }
                }
                div { class: "seg",
                    button {
                        class: if view_mode() == ViewMode::Cards { "active" } else { "" },
                        onclick: move |_| view_mode.set(ViewMode::Cards),
                        Icon { name: IconName::Grid, size: 12 }
                        " Cards"
                    }
                    button {
                        class: if view_mode() == ViewMode::Table { "active" } else { "" },
                        onclick: move |_| view_mode.set(ViewMode::Table),
                        Icon { name: IconName::Rows, size: 12 }
                        " Table"
                    }
                }
                span { class: "filter-count", "{filtered.len()} environments" }
            }

            if *loading.read() {
                div { class: "flex justify-center py-12",
                    div { class: "flex flex-col items-center gap-3",
                        div { class: "animate-spin rounded-full h-10 w-10 border-b-2 cf-spinner-accent" }
                        p { class: "text-sm {theme::text::SECONDARY}", "Loading environments…" }
                    }
                }
            } else if filtered.is_empty() {
                div { class: "empty", style: "margin:24px;", "No environments match." }
            } else if view_mode() == ViewMode::Cards {
                div { class: "cards-grid",
                    for env in filtered.iter() {
                        EnvironmentCard {
                            key: "{env.id}",
                            environment: env.clone(),
                            policy_library: policy_library_state.read().clone(),
                            on_edit: move |env| {
                                form_error.set(None);
                                form_draft.set(Some(form_draft_from_environment(&env)));
                            }
                        }
                    }
                }
            } else {
                EnvironmentTable {
                    environments: filtered.clone(),
                    policy_library: policy_library_state.read().clone(),
                    on_edit: move |env| {
                        form_error.set(None);
                        form_draft.set(Some(form_draft_from_environment(&env)));
                    }
                }
            }

            EnvironmentFormModal {
                draft: form_draft,
                existing: items.clone(),
                policy_library: policy_library_state.read().clone(),
                error: form_error,
                on_close: move |_| {
                    form_draft.set(None);
                    form_error.set(None);
                },
                on_remove: move |env| {
                    pending_remove.set(Some(env));
                },
                on_save: move |next: EnvironmentFormDraft| {
                    if let Err(err) = validate_environment_form(&next, &environments.read()) {
                        form_error.set(Some(err));
                        return;
                    }
                    save_environment_form(
                        next,
                        environments.clone(),
                        form_draft.clone(),
                        form_error.clone(),
                        api_notice.clone(),
                        default_required_policy,
                    );
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
                            let mut form_draft = form_draft.clone();
                            spawn(async move {
                                match delete_environment_via_api(environment_id).await {
                                    Ok(()) => {
                                        let mut values = environments.read().clone();
                                        values.retain(|item| item.id != environment_id);
                                        environments.set(values);
                                        pending_remove.set(None);
                                        form_draft.set(None);
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

#[derive(Clone, PartialEq)]
struct EnvironmentTotals {
    systems: usize,
    caches: usize,
    manual_policy: usize,
    auto_sync_off: usize,
}

impl EnvironmentTotals {
    fn from(items: &[EnvironmentItem]) -> Self {
        Self {
            systems: items.iter().map(|env| env.system_count).sum(),
            caches: items.iter().filter(|env| env.cache.is_some()).count(),
            manual_policy: items
                .iter()
                .filter(|env| env.default_policy == EnvironmentDeploymentPolicy::Manual)
                .count(),
            auto_sync_off: items.iter().filter(|env| !env.auto_sync).count(),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct StatStripProps {
    totals: EnvironmentTotals,
    total_tiers: usize,
}

#[component]
fn StatStrip(props: StatStripProps) -> Element {
    let cache_value = format!("{}/{}", props.totals.caches, props.total_tiers);
    rsx! {
        div { class: "stat-strip",
            StatCard { label: "Total tiers", value: props.total_tiers.to_string(), color: "#a78bfa" }
            StatCard { label: "Systems", value: props.totals.systems.to_string(), color: "#60a5fa" }
            StatCard { label: "Caches", value: cache_value, color: "#34d399" }
            StatCard { label: "Manual policy", value: props.totals.manual_policy.to_string(), color: "#fbbf24" }
            StatCard { label: "Auto-sync off", value: props.totals.auto_sync_off.to_string(), color: "#f87171" }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct StatCardProps {
    label: &'static str,
    value: String,
    color: &'static str,
}

#[component]
fn StatCard(props: StatCardProps) -> Element {
    rsx! {
        div { class: "stat",
            span { class: "stat-accent", style: "--stat-color:{props.color};" }
            div { class: "stat-label", "{props.label}" }
            div { class: "stat-value", "{props.value}" }
        }
    }
}

fn filtered_environments(items: &[EnvironmentItem], query: &str) -> Vec<EnvironmentItem> {
    let q = query.trim().to_ascii_lowercase();
    items
        .iter()
        .filter(|env| {
            q.is_empty()
                || env.name.to_ascii_lowercase().contains(&q)
                || env
                    .description
                    .as_ref()
                    .map(|description| description.to_ascii_lowercase().contains(&q))
                    .unwrap_or(false)
        })
        .cloned()
        .collect()
}

fn new_environment_form_draft(default_required_policy: Uuid) -> EnvironmentFormDraft {
    EnvironmentFormDraft {
        id: None,
        name: String::new(),
        description: String::new(),
        color_hex: "#2563eb".to_string(),
        required_policy_ids: vec![default_required_policy],
        default_policy: EnvironmentDeploymentPolicy::Manual,
        auto_sync: true,
        requires_approval: true,
        is_production: false,
    }
}

fn form_draft_from_environment(env: &EnvironmentItem) -> EnvironmentFormDraft {
    EnvironmentFormDraft {
        id: Some(env.id),
        name: env.name.clone(),
        description: env.description.clone().unwrap_or_default(),
        color_hex: env.color_hex.clone(),
        required_policy_ids: env.required_policy_ids.clone(),
        default_policy: env.default_policy,
        auto_sync: env.auto_sync,
        requires_approval: env.requires_approval,
        is_production: env.is_production,
    }
}

fn save_environment_form(
    next: EnvironmentFormDraft,
    mut environments: Signal<Vec<EnvironmentItem>>,
    mut form_draft: Signal<Option<EnvironmentFormDraft>>,
    mut form_error: Signal<Option<String>>,
    mut api_notice: Signal<Option<String>>,
    default_required_policy: Uuid,
) {
    spawn(async move {
        let result = if let Some(environment_id) = next.id {
            update_environment_via_api(
                environment_id,
                next.name.trim().to_string(),
                normalize_optional(&next.description),
                normalize_color_hex(&next.color_hex),
                default_required_policy,
            )
            .await
        } else {
            let draft = NewEnvironmentDraft {
                name: next.name.clone(),
                description: next.description.clone(),
                color_hex: next.color_hex.clone(),
                required_policy_ids: next.required_policy_ids.clone(),
            };
            if let Err(err) = validate_environment(&draft, &environments.read(), &policy_library())
            {
                form_error.set(Some(err));
                return;
            }
            create_environment_via_api(
                next.name.trim().to_string(),
                normalize_optional(&next.description),
                normalize_color_hex(&next.color_hex),
                true,
                default_required_policy,
            )
            .await
        };

        match result {
            Ok(mut saved) => {
                if let Err(message) =
                    update_environment_policies_via_api(saved.id, next.required_policy_ids.clone())
                        .await
                {
                    api_notice.set(Some(message));
                }
                saved.required_policy_ids = next.required_policy_ids;
                saved.default_policy = next.default_policy;
                saved.auto_sync = next.auto_sync;
                saved.requires_approval = next.requires_approval;
                saved.is_production = next.is_production;

                let mut values = environments.read().clone();
                if let Some(target) = values.iter_mut().find(|env| env.id == saved.id) {
                    *target = saved;
                } else {
                    values.push(saved);
                }
                values.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                environments.set(values);
                form_draft.set(None);
                form_error.set(None);
            }
            Err(message) => {
                api_notice.set(Some(message.clone()));
                form_error.set(Some(message));
            }
        }
    });
}
