//! Environments list view with CrystalForgelatest parity.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::alerts::{
    NAV_BADGES, acknowledge_with_cursor_and_ids, attention_row_class, dismiss_attention_item,
    should_flash,
};

use crate::components::environments::{
    EnvironmentCard, EnvironmentDeploymentPolicy, EnvironmentFormDraft, EnvironmentFormModal,
    EnvironmentItem, EnvironmentTable, NewEnvironmentDraft, PolicyOption, RemoveEnvironmentDialog,
    environment_name_for_id, normalize_color_hex, normalize_optional, policy_library,
    required_agent_policy_id, validate_environment, validate_environment_form,
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

fn environment_alert_occurrence_id(env: &EnvironmentItem) -> String {
    format!(
        "{}:{}:{}:{}",
        env.id, env.health.critical, env.health.offline, env.cve_critical_high
    )
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
            let attention_items = items
                .iter()
                .filter(|env| env.health.critical > 0 || env.health.offline > 0)
                .collect::<Vec<_>>();
            let attention_count = attention_items.len() as i64;
            let alert_ids = attention_items
                .iter()
                .map(|env| environment_alert_occurrence_id(env))
                .collect::<Vec<_>>();
            let ack_snapshot = {
                let badges = NAV_BADGES.read_unchecked();
                (
                    badges.observed_at.clone(),
                    badges.environments_fingerprint.clone(),
                )
            };
            let loaded_without_notice = result.notice.is_none() && policies_result.notice.is_none();
            environments.set(items);
            policy_library_state.set(policies_result.policies);

            api_notice.set(match (result.notice, policies_result.notice) {
                (Some(a), Some(b)) => Some(format!("{a}; {b}")),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            });
            loading.set(false);
            if loaded_without_notice {
                if let (Some(cursor), fingerprint) = ack_snapshot {
                    acknowledge_with_cursor_and_ids(
                        "environments",
                        attention_count,
                        cursor,
                        fingerprint,
                        Some(alert_ids),
                    );
                }
            }
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
    let mut view_env = use_signal(|| None::<EnvironmentItem>);

    let items = environments.read().clone();
    let filtered = filtered_environments(&items, &query());
    let totals = EnvironmentTotals::from(&items);

    // Attention flash for environments with critical/offline systems (TASK-385).
    let env_needs_attention =
        |env: &EnvironmentItem| -> bool { env.health.critical > 0 || env.health.offline > 0 };
    let attention_count = items.iter().filter(|e| env_needs_attention(e)).count() as i64;
    let mut flash_signal = use_signal(|| false);
    let flash_global = flash_signal();
    use_effect(move || {
        if should_flash("environments", attention_count > 0) {
            flash_signal.set(true);
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(3200).await;
                flash_signal.set(false);
            });
        }
    });

    // Pre-compute per-item flash booleans for cards/table (outside rsx! to avoid parse issues).
    let flashes: Vec<bool> = filtered
        .iter()
        .map(|env| flash_global && env_needs_attention(env))
        .collect();

    // Pre-compute per-item attention class strings for persistent red highlighting.
    let attention_classes: Vec<String> = filtered
        .iter()
        .map(|env| {
            let env_key = environment_alert_occurrence_id(env);
            let is_attention = env_needs_attention(env);
            let flash_now = flash_global && is_attention;
            attention_row_class("", "environments", &env_key, is_attention, flash_now)
        })
        .collect();

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
                    for (env, attention_class) in filtered.iter().zip(attention_classes.iter()) {
                        EnvironmentCard {
                            key: "{env.id}",
                            environment: env.clone(),
                            policy_library: policy_library_state.read().clone(),
                            flash: flash_global && env_needs_attention(env),
                            attention_class: attention_class.clone(),
                            on_view: move |env: EnvironmentItem| {
                                // Opening the detail panel counts as "visiting" the environment —
                                // dismiss its persistent attention row (same as edit did in TASK-385).
                                dismiss_attention_item("environments", &environment_alert_occurrence_id(&env));
                                view_env.set(Some(env));
                            },
                            on_edit: move |env: EnvironmentItem| {
                                dismiss_attention_item("environments", &environment_alert_occurrence_id(&env));
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
                    flashes: flashes.clone(),
                    attention_classes: attention_classes.clone(),
                    on_view: move |env: EnvironmentItem| {
                        // Opening the detail panel counts as "visiting" the environment.
                        dismiss_attention_item("environments", &environment_alert_occurrence_id(&env));
                        view_env.set(Some(env));
                    },
                    on_edit: move |env: EnvironmentItem| {
                        dismiss_attention_item("environments", &environment_alert_occurrence_id(&env));
                        form_error.set(None);
                        form_draft.set(Some(form_draft_from_environment(&env)));
                    }
                }
            }

            if let Some(env) = view_env.read().clone() {
                EnvPanel {
                    env: env.clone(),
                    on_close: move |_| view_env.set(None),
                    on_edit: move |env: EnvironmentItem| {
                        // dismiss from within panel too (in case user re-opens edit from panel
                        // without having clicked the card first).
                        dismiss_attention_item("environments", &environment_alert_occurrence_id(&env));
                        view_env.set(None);
                        form_error.set(None);
                        form_draft.set(Some(form_draft_from_environment(&env)));
                    },
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
                .filter(|env| env.default_policy == Some(EnvironmentDeploymentPolicy::Manual))
                .count(),
            auto_sync_off: items
                .iter()
                .filter(|env| env.auto_sync == Some(false))
                .count(),
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
        default_policy: None,
        auto_sync: None,
        requires_approval: None,
        is_production: None,
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
                saved.required_policy_ids = next.required_policy_ids.clone();
                // TASK-359/TASK-362 fields are read-only until backend support
                // lands. Do not copy modal display values into local state;
                // doing so would make non-persisted changes appear saved.

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

// ---------------------------------------------------------------------------
// EnvPanel — side panel showing environment details
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
struct EnvPanelProps {
    env: EnvironmentItem,
    on_close: EventHandler<()>,
    on_edit: EventHandler<EnvironmentItem>,
}

#[component]
fn EnvPanel(props: EnvPanelProps) -> Element {
    let env = props.env.clone();
    let env_for_edit = env.clone();
    let nav = use_navigator();
    let nav_for_cache = nav.clone();
    let nav_for_compliance = nav.clone();
    let total = env.health.total().max(env.system_count).max(1) as f64;
    let has_cache = env.cache.is_some();
    let cache_url = env
        .cache
        .as_ref()
        .map(|c| c.url.clone())
        .unwrap_or_default();
    let is_production = env.is_production.unwrap_or(false);
    let compliance_label = if is_production {
        "DISA STIG (placeholder)"
    } else {
        ""
    };
    let no_health = env.health.total() == 0;

    rsx! {
        // Backdrop — clicking outside closes the panel
        div {
            class: "side-panel-backdrop",
            onclick: move |_| props.on_close.call(()),
        }

        aside { class: "side-panel",
            // Panel header
            div { class: "panel-head",
                div { style: "display:flex; align-items:center; gap:8px; min-width:0;",
                    span { class: "env-dot", style: "background:{env.color_hex}; flex-shrink:0;" }
                    span { class: "panel-title", style: "min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                        "{env.name}"
                    }
                    if is_production {
                        span { class: "env-prod-badge",
                            Icon { name: IconName::Shield, size: 9 }
                            " PROD"
                        }
                    }
                }
                button {
                    class: "btn-icon focus-ring",
                    title: "Close",
                    onclick: move |_| props.on_close.call(()),
                    Icon { name: IconName::X, size: 14 }
                }
            }

            div { class: "panel-body",
                // Health bar section
                div { class: "panel-section",
                    div { style: "font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:0.06em; color:var(--cf-text-muted); margin-bottom:8px;",
                        "Health"
                    }
                    div { class: "env-health-bar", style: "height:10px; border-radius:5px; overflow:hidden; display:flex;",
                        if env.health.healthy > 0 {
                            div { style: "width:{pct_f(env.health.healthy, total)}%; background:#34d399;", title: "{env.health.healthy} healthy" }
                        }
                        if env.health.warning > 0 {
                            div { style: "width:{pct_f(env.health.warning, total)}%; background:#fbbf24;", title: "{env.health.warning} warning" }
                        }
                        if env.health.critical > 0 {
                            div { style: "width:{pct_f(env.health.critical, total)}%; background:#f87171;", title: "{env.health.critical} critical" }
                        }
                        if env.health.offline > 0 {
                            div { style: "width:{pct_f(env.health.offline, total)}%; background:#6b7280;", title: "{env.health.offline} offline" }
                        }
                        if no_health {
                            div { style: "width:100%; background:var(--cf-divider);" }
                        }
                    }
                    div { class: "env-health-legend", style: "margin-top:6px;",
                        if env.health.healthy > 0 {
                            span { span { class: "env-health-sw", style: "background:#34d399;" } "{env.health.healthy} healthy" }
                        }
                        if env.health.warning > 0 {
                            span { span { class: "env-health-sw", style: "background:#fbbf24;" } "{env.health.warning} warning" }
                        }
                        if env.health.critical > 0 {
                            span { span { class: "env-health-sw", style: "background:#f87171;" } "{env.health.critical} critical" }
                        }
                        if env.health.offline > 0 {
                            span { span { class: "env-health-sw", style: "background:#6b7280;" } "{env.health.offline} offline" }
                        }
                        if no_health {
                            span { style: "color:var(--cf-text-muted); font-size:11px;", "No health data" }
                        }
                    }
                }

                // Configuration section
                div { class: "panel-section",
                    div { style: "font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:0.06em; color:var(--cf-text-muted); margin-bottom:8px;",
                        "Configuration"
                    }
                    dl { class: "kv-grid",
                        dt { "Systems" }
                        dd { "{env.system_count}" }

                        dt { "Cache" }
                        dd {
                            if has_cache {
                                button {
                                    class: "btn-link focus-ring",
                                    style: "font-size:12px; cursor:pointer; color:var(--cf-brand-purple); background:none; border:none; padding:0; text-align:left;",
                                    onclick: move |_| {
                                        if let Some(window) = web_sys::window() {
                                            let encoded = js_sys::encode_uri_component(&cache_url)
                                                .as_string()
                                                .unwrap_or_else(|| cache_url.clone());
                                            let _ = window.location().set_href(&format!("/caches?focus={encoded}"));
                                        } else {
                                            nav_for_cache.push(Route::CachesView {});
                                        }
                                    },
                                    Icon { name: IconName::Link, size: 10 }
                                    " {cache_url}"
                                }
                            } else {
                                button {
                                    class: "btn-link focus-ring",
                                    style: "font-size:12px; cursor:pointer; color:var(--cf-text-muted); background:none; border:none; padding:0; text-align:left;",
                                    onclick: move |_| { nav_for_cache.push(Route::CachesView {}); },
                                    "not configured →"
                                }
                            }
                        }

                        dt { "Compliance" }
                        dd {
                            if is_production {
                                button {
                                    class: "btn-link focus-ring",
                                    style: "font-size:12px; cursor:pointer; color:var(--cf-brand-purple); background:none; border:none; padding:0; text-align:left;",
                                    onclick: move |_| { nav_for_compliance.push(Route::ComplianceView {}); },
                                    Icon { name: IconName::Shield, size: 10 }
                                    " {compliance_label}"
                                }
                            } else {
                                span { style: "font-size:12px; color:var(--cf-text-muted);", "none" }
                            }
                        }

                        dt { "Roles" }
                        dd {
                            if let Some(count) = env.role_assignment_count {
                                span { style: "font-size:12px;", "{count} assignments" }
                            } else {
                                span { style: "font-size:12px; color:var(--cf-text-muted);", title: "TASK-362 tracks persisted environment RBAC assignments", "not persisted" }
                            }
                        }
                    }
                }

                // Systems section — show count since full system list isn't loaded here
                div { class: "panel-section",
                    div { style: "font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:0.06em; color:var(--cf-text-muted); margin-bottom:8px;",
                        "Systems"
                    }
                    if env.system_count == 0 {
                        p { style: "font-size:12px; color:var(--cf-text-muted);", "No systems in this environment." }
                    } else {
                        p { style: "font-size:12px; color:var(--cf-text-secondary);",
                            span { style: "font-weight:600;", "{env.system_count}" }
                            " system{plural_s(env.system_count)} assigned"
                        }
                        if !env.flake_names.is_empty() {
                            div { style: "display:flex; flex-wrap:wrap; gap:4px; margin-top:6px;",
                                for flake in env.flake_names.iter().take(8) {
                                    span { class: "chip chip-unknown mono", style: "font-size:10px;", "{flake}" }
                                }
                                if env.flake_names.len() > 8 {
                                    span { class: "chip chip-unknown", style: "font-size:10px;", "+{env.flake_names.len() - 8} more" }
                                }
                            }
                        }
                    }
                }

                // Actions
                div { class: "panel-actions",
                    button {
                        class: "btn btn-subtle focus-ring",
                        style: "width:100%;",
                        onclick: move |_| props.on_edit.call(env_for_edit.clone()),
                        Icon { name: IconName::Gear, size: 13 }
                        " Edit environment"
                    }
                }
            }
        }
    }
}

fn pct_f(count: usize, total: f64) -> i32 {
    ((count as f64 / total) * 100.0).round() as i32
}

fn plural_s(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_env(name: &str) -> EnvironmentItem {
        EnvironmentItem {
            id: Uuid::from_u128(42),
            name: name.to_string(),
            description: None,
            color_hex: "#2563EB".to_string(),
            system_count: 2,
            required_policy_ids: Vec::new(),
            health: Default::default(),
            cve_critical_high: 0,
            flake_names: Vec::new(),
            default_policy: None,
            cache: None,
            auto_sync: None,
            requires_approval: None,
            is_production: None,
            role_assignment_count: None,
        }
    }

    #[test]
    fn totals_do_not_count_unpersisted_cache_or_policy_placeholders() {
        let items = vec![test_env("production")];
        let totals = EnvironmentTotals::from(&items);

        assert_eq!(totals.caches, 0);
        assert_eq!(totals.manual_policy, 0);
        assert_eq!(totals.auto_sync_off, 0);
    }

    #[test]
    fn form_save_helper_does_not_copy_placeholder_only_fields() {
        let mut saved = test_env("production");
        let draft = EnvironmentFormDraft {
            id: Some(saved.id),
            name: saved.name.clone(),
            description: String::new(),
            color_hex: saved.color_hex.clone(),
            required_policy_ids: vec![Uuid::from_u128(1)],
            default_policy: Some(EnvironmentDeploymentPolicy::AutoLatest),
            auto_sync: Some(false),
            requires_approval: Some(true),
            is_production: Some(true),
        };

        saved.required_policy_ids = draft.required_policy_ids.clone();

        assert_eq!(saved.required_policy_ids, vec![Uuid::from_u128(1)]);
        assert_eq!(saved.default_policy, None);
        assert_eq!(saved.auto_sync, None);
        assert_eq!(saved.requires_approval, None);
        assert_eq!(saved.is_production, None);
    }
}
