//! Environments list view with CrystalForgelatest parity.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::alerts::{
    NAV_BADGES, acknowledge_with_cursor_and_ids, attention_row_class, dismiss_attention_item,
    occurrence_id_for_subject, should_flash,
};
use crate::api::client::fetch_systems;
use crate::api::models::{HealthStatus, SortOrder, SystemsListParams};

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

#[component]
pub fn EnvironmentsListView() -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let is_admin_user = auth::is_admin(&app_state.read().auth);
    let config_health = app_state.read().config_health.clone();

    let mut policy_library_state = use_signal(policy_library);
    let default_required_policy = required_agent_policy_id(&policy_library_state.read());

    let mut environments = use_signal(Vec::<EnvironmentItem>::new);
    let mut bundle_catalog = use_signal(Vec::<crate::api::models::ComplianceBundleSummary>::new);
    let mut api_notice = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);
    let mut redirect_to_login = use_signal(|| false);
    let nav = use_navigator();

    use_effect(move || {
        spawn(async move {
            let policies_result = load_policies_with_fallback().await;
            let effective_default_policy = required_agent_policy_id(&policies_result.policies);
            let result = load_environments_with_fallback(effective_default_policy).await;
            // Load bundle catalog for the assignment picker (best-effort).
            if let Ok(bundles) = crate::api::client::fetch_compliance_bundles().await {
                bundle_catalog.set(bundles);
            }

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
            let ack_snapshot = {
                let badges = NAV_BADGES.read_unchecked();
                badges.observed_at.clone()
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
                if let Some(cursor) = ack_snapshot {
                    let occurrence_ids = NAV_BADGES
                        .read_unchecked()
                        .environments_occurrence_ids
                        .clone();
                    acknowledge_with_cursor_and_ids("environments", cursor, occurrence_ids);
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
    // Snapshot of bundle assignments at modal open — used to diff on Save.
    let mut original_assignments = use_signal(|| Vec::<crate::components::environments::EnvBundleAssignment>::new());
    let mut assignment_load_state = use_signal(|| crate::components::environments::AssignmentLoadState::Ready);
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
            // Resolve the same way dismiss_attention_item resolves its local
            // key: prefer the canonical server occurrence key (so a
            // recurrence after resolution is not permanently hidden by a
            // stale local entry), falling back to the stable environment id.
            // Never a composite of mutable critical/offline/CVE counts,
            // which changes on every rollup poll and would never match after
            // a dismiss.
            let env_id_str = env.id.to_string();
            let env_key =
                occurrence_id_for_subject("environments", &env_id_str).unwrap_or(env_id_str);
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
                    p { class: "page-subtitle", "{items.len()} tiers · {totals.systems} systems · {totals.caches} caches configured" }
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
                                dismiss_attention_item(
                                    "environments",
                                    &env.id.to_string(),
                                    occurrence_id_for_subject("environments", &env.id.to_string()).as_deref(),
                                );
                                view_env.set(Some(env));
                            },
                            on_edit: move |env: EnvironmentItem| {
                                dismiss_attention_item(
                                    "environments",
                                    &env.id.to_string(),
                                    occurrence_id_for_subject("environments", &env.id.to_string()).as_deref(),
                                );
                                form_error.set(None);
                                open_edit_modal(&env, form_draft.clone(), original_assignments.clone(), assignment_load_state.clone());
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
                        dismiss_attention_item(
                            "environments",
                            &env.id.to_string(),
                            occurrence_id_for_subject("environments", &env.id.to_string()).as_deref(),
                        );
                        view_env.set(Some(env));
                    },
                    on_edit: move |env: EnvironmentItem| {
                        dismiss_attention_item(
                            "environments",
                            &env.id.to_string(),
                            occurrence_id_for_subject("environments", &env.id.to_string()).as_deref(),
                        );
                        form_error.set(None);
                        open_edit_modal(&env, form_draft.clone(), original_assignments.clone(), assignment_load_state.clone());
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
                        dismiss_attention_item(
                            "environments",
                            &env.id.to_string(),
                            occurrence_id_for_subject("environments", &env.id.to_string()).as_deref(),
                        );
                        view_env.set(None);
                        form_error.set(None);
                        open_edit_modal(&env, form_draft.clone(), original_assignments.clone(), assignment_load_state.clone());
                    },
                }
            }

            EnvironmentFormModal {
                draft: form_draft,
                existing: items.clone(),
                policy_library: policy_library_state.read().clone(),
                bundle_catalog: bundle_catalog.read().clone(),
                assignment_load_state,
                on_retry_assignments: move |_| {
                    let current_id = form_draft.read().as_ref().and_then(|draft| draft.id);
                    if let Some(env) = current_id.and_then(|id| environments.read().iter().find(|env| env.id == id).cloned()) {
                        open_edit_modal(&env, form_draft.clone(), original_assignments.clone(), assignment_load_state.clone());
                    }
                },
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
                    let orig = original_assignments.read().clone();
                    save_environment_form(
                        next,
                        orig,
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
        // Matches the design's declared defaults for a freshly created
        // environment (docs/design/CrystalForge/components/EnvironmentsView.jsx):
        // manual deploys, auto-sync on, approval required, not production.
        default_policy: Some(EnvironmentDeploymentPolicy::Manual),
        auto_sync: Some(true),
        requires_approval: Some(true),
        is_production: Some(false),
        // Bundle assignments are loaded from the server when the modal opens.
        bundle_assignments: Vec::new(),
    }
}

/// Open the edit modal for an environment. The draft starts with empty
/// `bundle_assignments` (shown as loading); a background fetch then
/// populates them and snaps the `original_assignments` snapshot.
fn open_edit_modal(
    env: &EnvironmentItem,
    mut form_draft: Signal<Option<EnvironmentFormDraft>>,
    mut original_assignments: Signal<Vec<crate::components::environments::EnvBundleAssignment>>,
    mut assignment_load_state: Signal<crate::components::environments::AssignmentLoadState>,
) {
    let draft = form_draft_from_environment(env);
    let env_id = env.id;
    original_assignments.set(env.bundle_assignments.clone());
    form_draft.set(Some(draft.clone()));
    assignment_load_state.set(crate::components::environments::AssignmentLoadState::Loading);
    // Load authoritative assignments in the background.
    spawn(async move {
        let assignments_result = crate::environments::adapter::load_environment_bundle_assignments(&env_id).await;
        match assignments_result {
            Ok(loaded) => {
                original_assignments.set(loaded.clone());
                let cur = form_draft.read().clone();
                if let Some(mut current) = cur {
                    if current.id == Some(env_id) {
                        current.bundle_assignments = loaded;
                        form_draft.set(Some(current));
                        assignment_load_state.set(crate::components::environments::AssignmentLoadState::Ready);
                    }
                }
            }
            Err(err) => {
                assignment_load_state.set(crate::components::environments::AssignmentLoadState::Failed(err.clone()));
                web_sys::console::warn_1(&format!("Failed to load environment assignments: {err}").into());
            }
        }
    });
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
        // bundle_assignments is populated from the server when the modal opens,
        // not from EnvironmentItem which only has a legacy single-bundle summary.
        bundle_assignments: env.bundle_assignments.clone(),
    }
}

fn save_environment_form(
    next: EnvironmentFormDraft,
    original_assignments: Vec<crate::components::environments::EnvBundleAssignment>,
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
                next.default_policy,
                next.auto_sync,
                next.requires_approval,
                next.is_production,
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
                next.default_policy,
                next.auto_sync,
                next.requires_approval,
                next.is_production,
                default_required_policy,
            )
            .await
        };

        match result {
            Ok(mut saved) => {
                // Update gate policies.
                if let Err(message) =
                    update_environment_policies_via_api(saved.id, next.required_policy_ids.clone())
                        .await
                {
                    // Keep the modal open; report the failure without closing.
                    form_error.set(Some(format!(
                        "Environment saved but gate policies could not be updated: {message}"
                    )));
                    api_notice.set(Some(message));
                    return;
                }

                // Reconcile bundle assignments. On failure keep modal open.
                if let Err(message) =
                    crate::environments::adapter::reconcile_environment_assignments(
                        saved.id,
                        &original_assignments,
                        &next.bundle_assignments,
                    )
                    .await
                {
                    form_error.set(Some(format!(
                        "Environment saved but bundle assignment update failed: {message}"
                    )));
                    return;
                }

                saved.required_policy_ids = next.required_policy_ids.clone();
                saved.bundle_assignments = next.bundle_assignments.clone();

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

#[derive(Props, Clone, PartialEq)]
struct EnvPanelSystemsProps {
    env_name: String,
    expected_count: usize,
}

#[component]
fn EnvPanel(props: EnvPanelProps) -> Element {
    let env = props.env.clone();
    let env_for_edit = env.clone();
    let description_text = env.description.clone();
    let display_policy = env.default_policy;
    let display_auto_sync = env.auto_sync;
    let display_requires_approval = env.requires_approval;
    let display_role_assignment_count = env.role_assignment_count;
    let display_cache = env.cache.clone();
    let display_gate_count = if env.required_policy_ids.is_empty() {
        None
    } else {
        Some(env.required_policy_ids.len())
    };
    let display_compliance_label = env
        .compliance_bundle
        .as_ref()
        .map(|bundle| bundle.framework.clone());
    let display_bundle_assignments = env.bundle_assignments.clone();
    let is_production = env.is_production.unwrap_or(false);
    let total = env.health.total().max(env.system_count).max(1) as f64;
    let no_health = env.health.total() == 0;
    let nav = use_navigator();
    let cache_nav = nav.clone();
    let cache_url = display_cache
        .as_ref()
        .map(|cache| cache.url.clone())
        .unwrap_or_default();
    let cache_url_for_label = cache_url.clone();
    let cache_url_for_title = cache_url.clone();

    rsx! {
        div {
            class: "side-panel-backdrop",
            onclick: move |_| props.on_close.call(()),
        }

        aside { class: "side-panel", role: "dialog", aria_modal: "true",
            div { class: "panel-head",
                div { class: "panel-title",
                    h2 {
                        span { class: "env-dot", style: "background:{env.color_hex};" }
                        "{env.name}"
                        if is_production {
                            span { class: "env-prod-badge",
                                Icon { name: IconName::Shield, size: 9 }
                                " PROD"
                            }
                        }
                    }
                    if let Some(description) = description_text {
                        span { class: "fqdn", "{description}" }
                    }
                }
                button {
                    class: "btn-icon focus-ring",
                    onclick: move |_| props.on_close.call(()),
                    aria_label: "Close",
                    Icon { name: IconName::X, size: 16 }
                }
            }

            div { class: "panel-body",
                if display_policy.is_some()
                    || display_auto_sync.is_some()
                    || display_requires_approval.is_some()
                {
                    section { class: "panel-section",
                        div { style: "display:flex; gap:8px; flex-wrap:wrap;",
                            if let Some(policy) = display_policy {
                                span {
                                    class: "chip {policy_chip_class(policy)}",
                                    "{policy.label()}"
                                }
                            }
                            if let Some(auto_sync) = display_auto_sync {
                                {
                                    let auto_sync_class = if auto_sync { "chip-healthy" } else { "chip-unknown" };
                                    rsx! {
                                        span {
                                            class: "chip {auto_sync_class}",
                                            if auto_sync { "auto-sync on" } else { "auto-sync off" }
                                        }
                                    }
                                }
                            }
                            if let Some(requires_approval) = display_requires_approval {
                                {
                                    let approval_class = if requires_approval { "chip-warning" } else { "chip-healthy" };
                                    rsx! {
                                        span {
                                            class: "chip {approval_class}",
                                            if requires_approval { "approval required" } else { "no approval needed" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                section { class: "panel-section",
                    h3 { "Health" }
                    div { class: "env-health-bar", style: "margin-bottom:8px;",
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
                    div { class: "env-health-legend",
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
                        if env.cve_critical_high > 0 {
                            span { style: "margin-left:auto;",
                                Icon { name: IconName::Shield, size: 10 }
                                " {env.cve_critical_high} CVE"
                            }
                        }
                    }
                }

                section { class: "panel-section",
                    h3 { "Configuration" }
                    dl { class: "kv-grid",
                        dt { "Cache" }
                        dd {
                            if display_cache.is_some() {
                                button {
                                    class: "sd-commit-sha-link",
                                    style: "background:none; border:none; padding:0; margin:0; text-align:left;",
                                    onclick: move |_| {
                                        if let Some(window) = web_sys::window() {
                                            let encoded = js_sys::encode_uri_component(&cache_url)
                                                .as_string()
                                                .unwrap_or_else(|| cache_url.clone());
                                            let _ = window.location().set_href(&format!("/caches?focus={encoded}"));
                                        } else {
                                            cache_nav.push(Route::CachesView {});
                                        }
                                    },
                                    title: "Open {cache_url_for_title} in Caches",
                                    Icon { name: IconName::Download, size: 10 }
                                    " {cache_url_for_label}"
                                }
                            } else {
                                span { style: "color:var(--cf-text-muted); font-style:italic;", "not configured" }
                            }
                        }

                        dt { "Enforcement" }
                        dd {
                            div { style: "display:flex; flex-direction:column; gap:6px;",
                                // Show all versioned bundle assignments if available.
                                if !display_bundle_assignments.is_empty() {
                                    for a in display_bundle_assignments.iter().cloned().collect::<Vec<_>>() {
                                        div { style: "display:flex; align-items:center; gap:6px;",
                                            button {
                                                class: "chip chip-info sd-commit-sha-link",
                                                style: "background:unset;",
                                                title: "{a.bundle_name} · {a.bundle_version} — open Compliance",
                                                onclick: move |_| { nav.push(Route::ComplianceView {}); },
                                                Icon { name: IconName::Shield, size: 9 }
                                                " {a.framework}"
                                            }
                                            span { class: "mono", style: "font-size:10px; color:var(--cf-text-muted);", "{a.bundle_version}" }
                                            if a.enforcement_mode == "report_only" {
                                                span { class: "chip chip-unknown", style: "font-size:10px;", "report only" }
                                            }
                                        }
                                    }
                                } else if let Some(compliance_label) = display_compliance_label.clone() {
                                    // Legacy single-bundle fallback.
                                    button {
                                        class: "chip chip-info sd-commit-sha-link",
                                        style: "background:unset;",
                                        title: "Open Compliance",
                                        onclick: move |_| { nav.push(Route::ComplianceView {}); },
                                        Icon { name: IconName::Shield, size: 9 }
                                        " {compliance_label}"
                                    }
                                }
                                if let Some(gate_count) = display_gate_count {
                                    span { class: "chip chip-unknown",
                                        "{gate_count} gate{plural_s(gate_count)}"
                                    }
                                }
                                if display_bundle_assignments.is_empty() && display_compliance_label.is_none() && display_gate_count.is_none() {
                                    span { style: "font-size:11px; color:var(--cf-text-muted);", "none" }
                                }
                            }
                        }

                        dt { "Role assignments" }
                        dd {
                            if let Some(count) = display_role_assignment_count {
                                "{count}"
                            } else {
                                span { style: "color:var(--cf-text-muted);", "not set" }
                            }
                        }
                    }
                }

                section { class: "panel-section",
                    h3 { "Flakes in use" }
                    if env.flake_names.is_empty() {
                        div { style: "font-size:12px; color:var(--cf-text-muted);", "none deployed" }
                    } else {
                        div { style: "display:flex; gap:6px; flex-wrap:wrap;",
                            for flake in &env.flake_names {
                                span { class: "chip chip-unknown mono", style: "font-size:11px;", "{flake}" }
                            }
                        }
                    }
                }

                EnvPanelSystems {
                    env_name: env.name.clone(),
                    expected_count: env.system_count,
                }
            }

            div { class: "panel-actions",
                button {
                    class: "btn btn-primary focus-ring",
                    onclick: move |_| props.on_edit.call(env_for_edit.clone()),
                    Icon { name: IconName::Gear, size: 12 }
                    " Edit environment"
                }
            }
        }
    }
}

#[component]
fn EnvPanelSystems(props: EnvPanelSystemsProps) -> Element {
    let nav = use_navigator();
    let env_name = props.env_name.clone();
    let systems = use_resource(move || {
        let env_name = env_name.clone();
        async move {
            fetch_systems(&SystemsListParams {
                page: Some(1),
                per_page: Some(200),
                search: None,
                health_status: None,
                deployment_status: None,
                environment: Some(env_name),
                sort_by: Some("hostname".to_string()),
                sort_order: Some(SortOrder::Asc),
            })
            .await
            .map(|response| response.items)
        }
    });

    rsx! {
        section { class: "panel-section",
            h3 { "Systems ({props.expected_count})" }
            match systems.read().as_ref() {
                None => rsx! {
                    if props.expected_count == 0 {
                        div { style: "font-size:12px; color:var(--cf-text-muted);", "No systems in this environment yet." }
                    } else {
                        div { style: "font-size:12px; color:var(--cf-text-muted);", "Loading systems…" }
                    }
                },
                Some(Ok(items)) if !items.is_empty() => rsx! {
                    div { style: "display:flex; flex-direction:column; gap:6px;",
                        for system in items.iter().take(8) {
                            button {
                                class: "sd-commit-sha-link",
                                style: "display:flex; align-items:center; gap:8px; font-size:12.5px; padding:3px 4px; margin:-3px -4px; background:none; border:none; width:100%; text-align:left;",
                                onclick: {
                                    let nav = nav.clone();
                                    let system_id = system.id.to_string();
                                    move |_| {
                                        nav.push(Route::SystemDetailView { id: system_id.clone() });
                                    }
                                },
                                span { class: "status-dot", style: "--status-color: {system_status_color(&system.health_status)};" }
                                span { class: "mono truncate", style: "flex:1; text-align:left;", "{system.hostname}" }
                            }
                        }
                        if items.len() > 8 {
                            div { style: "font-size:11px; color:var(--cf-text-muted);", "+{items.len() - 8} more" }
                        }
                    }
                },
                Some(Ok(_)) => rsx! {
                    div { style: "font-size:12px; color:var(--cf-text-muted);", "No systems in this environment yet." }
                },
                Some(Err(_)) => rsx! {
                    if props.expected_count == 0 {
                        div { style: "font-size:12px; color:var(--cf-text-muted);", "No systems in this environment yet." }
                    } else {
                        div { style: "font-size:12px; color:var(--cf-text-secondary);",
                            span { style: "font-weight:600;", "{props.expected_count}" }
                            " system{plural_s(props.expected_count)} assigned"
                        }
                    }
                }
            }
        }
    }
}

fn policy_chip_class(policy: EnvironmentDeploymentPolicy) -> &'static str {
    match policy {
        EnvironmentDeploymentPolicy::Manual => "chip-warning",
        EnvironmentDeploymentPolicy::AutoLatest => "chip-healthy",
        EnvironmentDeploymentPolicy::Pinned => "chip-unknown",
    }
}

fn system_status_color(status: &HealthStatus) -> &'static str {
    match status {
        HealthStatus::Healthy => "#34d399",
        HealthStatus::Warning => "#fbbf24",
        HealthStatus::Critical => "#f87171",
        HealthStatus::Offline => "#6b7280",
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
            compliance_bundle: None,
            bundle_assignments: Vec::new(),
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
            bundle_assignments: Vec::new(),
        };

        saved.required_policy_ids = draft.required_policy_ids.clone();

        assert_eq!(saved.required_policy_ids, vec![Uuid::from_u128(1)]);
        assert_eq!(saved.default_policy, None);
        assert_eq!(saved.auto_sync, None);
        assert_eq!(saved.requires_approval, None);
        assert_eq!(saved.is_production, None);
    }
}
