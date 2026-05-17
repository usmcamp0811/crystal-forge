//! Flakes list view with table/card toggle.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
#[cfg(target_arch = "wasm32")]
use js_sys::Object;
#[cfg(target_arch = "wasm32")]
use js_sys::{Function, Reflect};
use uuid::Uuid;
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::Closure;
#[cfg(target_arch = "wasm32")]
use web_sys::console;
use web_sys::{Node, window};

use crate::api::client::{
    ApiClientError, accept_flake_history_rewrite, create_flake, delete_flake,
    delete_flake_credentials, fetch_commit_diff, fetch_cve_scan_status, fetch_environments,
    fetch_flake_credentials, fetch_flake_timeline_for_tray, fetch_flake_timelines,
    fetch_flake_timelines_for_ids, fetch_flakes, put_flake_credentials, request_sync_all_flakes,
    request_sync_flake, test_flake_credentials, trigger_flake_config_cve_scan, update_flake,
};
use crate::api::models::{
    BuildStatus as ApiBuildStatus, CreateFlakeCredentialRequest, CreateFlakeRequest,
    EnvironmentSummary, FlakeCommitSystemPath, FlakeRegistryItem, FlakeTimeline,
    TestFlakeCredentialRequest, UpdateFlakeRequest,
};
use crate::components::layout::Card;
use crate::components::notifications::{AlertBanner, AlertSeverity};
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::theme;
use crate::views::systems_mock::mock_system_details;

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

const VIEW_PREF_KEY: &str = "crystal_forge.flakes.view";
const FLAKE_TABLE_SCHEMA_NOTE: &str = "flakes(name, repo_url UNIQUE, branch)";
const INITIAL_TIMELINE_FLAKES: usize = 1;
const TIMELINE_BATCH_SIZE: usize = 2;
const MAX_SYSTEM_CHIPS_RENDER: usize = 24;
const MAX_SYSTEMS_STORED_PER_COMMIT: usize = 120;
const MAX_SYSTEM_LABEL_CHARS: usize = 96;
const MAX_WS_STREAM_SYSTEMS: usize = 80;

fn preview_systems(systems: &[String]) -> &[String] {
    let end = systems.len().min(MAX_SYSTEM_CHIPS_RENDER);
    &systems[..end]
}

fn truncate_system_label(raw: &str) -> String {
    let trimmed = raw.trim();
    let mut iter = trimmed.chars();
    let short: String = iter.by_ref().take(MAX_SYSTEM_LABEL_CHARS).collect();
    if iter.next().is_some() {
        format!("{short}...")
    } else {
        short
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlakesViewMode {
    Table,
    Cards,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilterDropdown {
    Environment,
    Commit,
    Size,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommitFilter {
    HasCommit,
    NoCommit,
}

impl CommitFilter {
    fn label(self) -> &'static str {
        match self {
            CommitFilter::HasCommit => "Has latest commit",
            CommitFilter::NoCommit => "No commits",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SizeBucket {
    Small,
    Medium,
    Large,
}

impl SizeBucket {
    fn label(self) -> &'static str {
        match self {
            SizeBucket::Small => "1-3 systems",
            SizeBucket::Medium => "4-9 systems",
            SizeBucket::Large => "10+ systems",
        }
    }

    fn matches(self, system_count: usize) -> bool {
        match self {
            SizeBucket::Small => (1..=3).contains(&system_count),
            SizeBucket::Medium => (4..=9).contains(&system_count),
            SizeBucket::Large => system_count >= 10,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortColumn {
    Name,
    Repo,
    Systems,
    Environments,
    LatestCommit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    fn toggle(self) -> Self {
        match self {
            SortDirection::Asc => SortDirection::Desc,
            SortDirection::Desc => SortDirection::Asc,
        }
    }
}

impl FlakesViewMode {
    fn from_storage(value: Option<String>) -> Self {
        match value.as_deref() {
            Some("cards") => Self::Cards,
            _ => Self::Table,
        }
    }

    fn as_storage(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Cards => "cards",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct FlakeListItem {
    id: i32,
    name: String,
    repo_url: String,
    branch: String,
    build_scope: String,
    latest_commit: Option<String>,
    system_count: usize,
    environments: Vec<String>,
    last_synced_at: DateTime<Utc>,
}

impl FlakeListItem {
    fn from_registry(item: FlakeRegistryItem) -> Self {
        Self {
            id: item.id,
            name: item.name,
            repo_url: item.repo_url,
            branch: item.branch,
            build_scope: item.build_scope,
            latest_commit: None,
            system_count: item.system_count.max(0) as usize,
            environments: Vec::new(),
            last_synced_at: Utc::now(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct FlakeHistoryCommit {
    id: i32,
    hash: String,
    message: String,
    author: String,
    committed_at: DateTime<Utc>,
    files_changed: usize,
    insertions: usize,
    deletions: usize,
    diff: String,
    systems: Vec<String>,
    system_paths: Vec<FlakeCommitSystemPath>,
    total_system_count: usize,
    build_status: Option<ApiBuildStatus>,
    evaluation_status: Option<String>,
    evaluation_error_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct ParsedDiffFile {
    old_path: String,
    new_path: String,
    language: &'static str,
    lines: Vec<RenderedDiffLine>,
}

#[derive(Clone, Debug, PartialEq)]
struct RenderedDiffLine {
    old_number: Option<usize>,
    new_number: Option<usize>,
    prefix: char,
    content: String,
    class_name: &'static str,
    is_hunk_header: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct NewFlakeDraft {
    name: String,
    repo_url: String,
    branch: String,
    build_scope: String,
    credential_type: String,
    credential_username: String,
    credential_secret: String,
    credential_ssh_username: String,
}

#[derive(Clone, Debug, PartialEq)]
struct EditFlakeDraft {
    id: i32,
    name: String,
    repo_url: String,
    branch: String,
    environment: String,
    description: String,
    build_scope: String,
    credential_type: String,
    credential_username: String,
    credential_secret: String,
    credential_ssh_username: String,
    has_existing_secret: bool,
}

/// Flakes list with toggles and filters.
#[component]
pub fn FlakesListView() -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let is_admin_user = auth::is_admin(&app_state.read().auth);

    // Shared config health (admin only) — used for flake eval error banner.
    let config_health = app_state.read().config_health.clone();

    let stored_view = LocalStorage::get::<String>(VIEW_PREF_KEY).ok();
    let mut view_mode = use_signal(|| FlakesViewMode::from_storage(stored_view));
    let query_view = prefers_view_from_query();
    let open_dropdown = use_signal(|| None::<FilterDropdown>);
    let container_id = use_memo(|| format!("flakes-filters-{}", Uuid::new_v4()));

    use_effect(move || {
        if let Some(mode) = query_view {
            view_mode.set(mode);
            let _ = LocalStorage::set(VIEW_PREF_KEY, mode.as_storage());
        }
    });

    {
        let mut open_dropdown = open_dropdown.clone();
        let container_id = container_id.clone();
        use_effect(move || {
            let Some(window) = window() else {
                return;
            };
            let Some(document) = window.document() else {
                return;
            };
            let document_for_listener = document.clone();
            let handler = Closure::<dyn FnMut(_)>::new(move |event: web_sys::Event| {
                if open_dropdown.read().is_none() {
                    return;
                }
                let target = match event.target() {
                    Some(target) => target,
                    None => return,
                };
                let node: Node = match target.dyn_into() {
                    Ok(node) => node,
                    Err(_) => return,
                };
                let container_id = container_id.read();
                if let Some(container) =
                    document_for_listener.get_element_by_id(container_id.as_str())
                {
                    if !container.contains(Some(&node)) {
                        open_dropdown.set(None);
                    }
                }
            });
            let _ = document
                .add_event_listener_with_callback("mousedown", handler.as_ref().unchecked_ref());
            handler.forget();
        });
    }

    let search = use_signal(String::new);
    let environment_filter = use_signal(Vec::<String>::new);
    let commit_filter = use_signal(Vec::<CommitFilter>::new);
    let size_filter = use_signal(Vec::<SizeBucket>::new);
    let mut flakes = use_signal(Vec::<FlakeListItem>::new);
    let loading_flakes = use_signal(|| true);
    let server_notice = use_signal(|| None::<String>);
    let mut flake_timelines = use_signal(Vec::<FlakeTimeline>::new);
    let mut show_add_form = use_signal(|| false);
    let mut add_error = use_signal(|| None::<String>);
    let mut editing_flake = use_signal(|| None::<EditFlakeDraft>);
    let mut edit_error = use_signal(|| None::<String>);
    let mut draft = use_signal(|| NewFlakeDraft {
        name: String::new(),
        repo_url: String::new(),
        branch: String::new(),
        build_scope: "cf_systems_only".to_string(),
        credential_type: "none".to_string(),
        credential_username: String::new(),
        credential_secret: String::new(),
        credential_ssh_username: String::new(),
    });
    let mut pending_remove = use_signal(|| None::<FlakeListItem>);
    let mut refreshing_flake = use_signal(|| None::<i32>);
    let mut selected_history_flake = use_signal(|| None::<i32>);
    let mut selected_history_commit = use_signal(|| None::<String>);
    let mut sync_note = use_signal(|| None::<String>);
    let mut last_manual_sync = use_signal(|| None::<DateTime<Utc>>);
    let mut rewrite_prompt = use_signal(|| None::<(i32, String, String)>);
    let mut cve_scan_status = use_signal(HashMap::<String, String>::new);

    let current_flakes = flakes.read().clone();
    let environments = unique_environments(&current_flakes);
    
    // Fetch environments from database for edit dialog
    let db_environments_resource = use_resource(|| async { fetch_environments().await });
    let db_environments: Vec<EnvironmentSummary> = match db_environments_resource.read().as_ref() {
        Some(Ok(envs)) => envs.clone(),
        _ => Vec::new(),
    };

    let filtered_flakes: Vec<FlakeListItem> = current_flakes
        .into_iter()
        .filter(|flake| matches_environment(flake, &environment_filter.read()))
        .filter(|flake| matches_commit_state(flake, &commit_filter.read()))
        .filter(|flake| matches_size(flake, &size_filter.read()))
        .filter(|flake| matches_search(flake, &search.read()))
        .collect();
    let sync_timestamp =
        (*last_manual_sync.read()).map(|ts| ts.format("%Y-%m-%d %H:%M:%S UTC").to_string());

    {
        let filtered_ids: Vec<i32> = filtered_flakes.iter().map(|flake| flake.id).collect();
        let mut selected_history_flake = selected_history_flake.clone();
        let mut selected_history_commit = selected_history_commit.clone();
        use_effect(move || {
            let current = *selected_history_flake.read();
            let has_selected = current
                .map(|id| filtered_ids.iter().any(|value| *value == id))
                .unwrap_or(false);
            if !has_selected {
                selected_history_flake.set(filtered_ids.first().copied());
                selected_history_commit.set(None);
            }
        });
    }

    {
        let mut flakes = flakes.clone();
        let mut loading_flakes = loading_flakes.clone();
        let mut server_notice = server_notice.clone();
        use_effect(move || {
            spawn(async move {
                match fetch_flakes().await {
                    Ok(items) => {
                        flakes.set(
                            items
                                .into_iter()
                                .map(FlakeListItem::from_registry)
                                .collect(),
                        );
                        server_notice.set(None);
                    }
                    Err(error) => {
                        server_notice.set(Some(format!("Flake API unavailable: {error}")));
                    }
                }
                loading_flakes.set(false);
            });
        });
    }

    // Load flake timelines
    {
        let mut flake_timelines = flake_timelines.clone();
        let flakes = flakes.clone();
        use_effect(move || {
            let flake_ids: Vec<i32> = flakes.read().iter().map(|flake| flake.id).collect();
            spawn(async move {
                if flake_ids.is_empty() {
                    flake_timelines.set(Vec::new());
                    return;
                }

                let initial_ids: Vec<i32> = flake_ids
                    .iter()
                    .take(INITIAL_TIMELINE_FLAKES)
                    .copied()
                    .collect();

                let mut merged_timelines = Vec::new();

                if !initial_ids.is_empty() {
                    match fetch_flake_timelines_for_ids(&initial_ids).await {
                        Ok(timelines) => {
                            merged_timelines = merge_flake_timeline_batches(
                                merged_timelines,
                                timelines,
                                &flake_ids,
                            );
                            flake_timelines.set(merged_timelines.clone());
                        }
                        Err(_error) => {
                            // Fallback to full fetch if subset request fails for any reason.
                            match fetch_flake_timelines().await {
                                Ok(timelines) => {
                                    flake_timelines.set(timelines);
                                }
                                Err(_) => {
                                    flake_timelines.set(Vec::new());
                                }
                            }
                            return;
                        }
                    }
                }

                let remaining_ids: Vec<i32> = flake_ids
                    .iter()
                    .skip(INITIAL_TIMELINE_FLAKES)
                    .copied()
                    .collect();

                for chunk in remaining_ids.chunks(TIMELINE_BATCH_SIZE) {
                    match fetch_flake_timelines_for_ids(chunk).await {
                        Ok(timelines) => {
                            merged_timelines = merge_flake_timeline_batches(
                                merged_timelines,
                                timelines,
                                &flake_ids,
                            );
                            flake_timelines.set(merged_timelines.clone());
                        }
                        Err(_) => {
                            // Keep already-loaded timelines if a later batch fails.
                        }
                    }
                }
            });
        });
    }

    let from_setup = use_signal(came_from_setup);
    let mut dismiss_add_target_callout = use_signal(|| false);

    rsx! {
        div {
            class: "space-y-6",
            id: "{container_id}",

            if from_setup() {
                div {
                    "data-testid": "setup-coach-flakes-callout",
                    style: "background:rgba(30,58,138,0.22); border:1px solid rgba(96,165,250,0.55); border-radius:8px; padding:12px 16px;",
                    p { style: "color:#dbeafe; font-size:12px; font-weight:700; margin:0; letter-spacing:0.03em; text-transform:uppercase;", "Setup Tour - Step 2 of 6" }
                    p { style: "color:#dbeafe; font-size:14px; font-weight:600; margin:4px 0 0 0;", "Register a flake source" }
                    p { style: "color:#bfdbfe; font-size:13px; margin:4px 0 0 0;", "Use Add Flake to track the repo and branch your systems should deploy." }
                }
            }

            header {
                class: "flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Flake Registry" }
                    p { class: "text-sm {theme::text::SECONDARY}", "Track flake repositories and deployment coverage." }
                }
                div {
                    class: "flex items-center gap-3",
                    button {
                        class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::DANGER_BTN} {theme::interactive::FOCUS_RING}",
                        onclick: move |_| {
                            let selected_flake_id = *selected_history_flake.read();
                            let mut flakes_signal = flakes.clone();
                            let mut timelines_signal = flake_timelines.clone();
                            let mut selected_history_commit = selected_history_commit.clone();
                            let mut last_manual_sync = last_manual_sync.clone();
                            let mut sync_note = sync_note.clone();
                            let mut rewrite_prompt = rewrite_prompt.clone();
                            let flakes_snapshot = flakes.read().clone();
                            spawn(async move {
                                let sync_result = if let Some(flake_id) = selected_flake_id {
                                    request_sync_flake(flake_id).await
                                } else {
                                    request_sync_all_flakes().await
                                };

                                match sync_result {
                                    Ok(response) => {
                                        let mut refresh_warning = false;
                                        match fetch_flakes().await {
                                            Ok(items) => {
                                                flakes_signal.set(
                                                    items
                                                        .into_iter()
                                                        .map(FlakeListItem::from_registry)
                                                        .collect(),
                                                );
                                            }
                                            Err(_) => {
                                                refresh_warning = true;
                                            }
                                        }

                                        match fetch_flake_timelines().await {
                                            Ok(timelines) => {
                                                timelines_signal.set(timelines);
                                                selected_history_commit.set(None);
                                            }
                                            Err(_) => {
                                                refresh_warning = true;
                                            }
                                        }
                                        last_manual_sync.set(Some(Utc::now()));
                                        let message = if refresh_warning {
                                            format!(
                                                "{} UI refresh was partial; reload if data looks stale.",
                                                response.message
                                            )
                                        } else {
                                            response.message
                                        };
                                        sync_note.set(Some(message));
                                    }
                                    Err(error) => {
                                        if let Some((flake_id, detail)) =
                                            extract_history_rewrite_conflict(&error, selected_flake_id)
                                        {
                                            let flake_name = flakes_snapshot
                                                .iter()
                                                .find(|f| f.id == flake_id)
                                                .map(|f| f.name.clone())
                                                .unwrap_or_else(|| format!("flake #{flake_id}"));
                                            rewrite_prompt.set(Some((flake_id, flake_name, detail)));
                                            sync_note.set(Some(
                                                "Sync blocked: git history rewrite detected. Review and accept rewrite to continue.".to_string(),
                                            ));
                                            return;
                                        }

                                        #[cfg(target_arch = "wasm32")]
                                        web_sys::console::error_1(
                                            &format!(
                                                "[CF] sync request failed for selected_flake_id={:?}: {}",
                                                selected_flake_id, error
                                            )
                                            .into(),
                                        );

                                        sync_note.set(Some(format!(
                                            "Sync failed for {}: {}",
                                            selected_flake_id
                                                .map(|id| format!("flake #{id}"))
                                                .unwrap_or_else(|| "all flakes".to_string()),
                                            error
                                        )));
                                        return;
                                    }
                                }
                            });
                        },
                        "Sync from Source"
                    }
                    div {
                        class: "relative z-[2101]",
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
                            if *show_add_form.read() {
                                "Close"
                            } else {
                                "Add Flake"
                            }
                        }
                        if from_setup() && !*show_add_form.read() && !dismiss_add_target_callout() {
                            div {
                                "data-testid": "setup-coach-flakes-target-callout",
                                style: "position:absolute; z-index:2200; right:0; top:calc(100% + 10px); background:rgba(30,64,175,0.94); border:1px solid rgba(96,165,250,0.75); border-radius:10px; padding:8px 10px; color:#dbeafe; font-size:12px; width:220px; box-shadow:0 10px 24px rgba(15,23,42,0.45);",
                                div {
                                    style: "position:absolute; top:-6px; right:18px; width:10px; height:10px; background:rgba(30,64,175,0.94); border-left:1px solid rgba(96,165,250,0.75); border-top:1px solid rgba(96,165,250,0.75); transform:rotate(45deg);"
                                }
                                p { style: "margin:0; color:#eff6ff; font-weight:600;", "Next action" }
                                p { style: "margin:2px 0 0 0;", "Click Add Flake to register your source repository." }
                            }
                        }
                    }
                    ViewToggle {
                        view_mode: *view_mode.read(),
                        on_change: move |mode| {
                            view_mode.set(mode);
                            let _ = LocalStorage::set(VIEW_PREF_KEY, mode.as_storage());
                        }
                    }
                }
            }

            if let Some(note) = sync_note.read().clone() {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-blue-100 cf-chip-info",
                    "{note}"
                }
            } else if let Some(sync_ts) = sync_timestamp {
                p {
                    class: "text-xs {theme::text::MUTED}",
                    "Last manual sync {sync_ts}"
                }
            }

            if let Some(message) = server_notice.read().clone() {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-amber-100 cf-chip-warning",
                    "{message}"
                }
            }

            // Admin-only: warn when any flake has eval errors on its latest commit.
            if is_admin_user {
                if let Some(ref health) = config_health {
                    if health.checks.iter().any(|c| c.id == "flake_eval_errors" && !c.passed) {
                        AlertBanner {
                            severity: AlertSeverity::Warning,
                            message: "One or more flakes have evaluation errors on their latest commit. Check flake configuration and commit history.".to_string(),
                        }
                    }
                }
            }

            if *show_add_form.read() {
                AddFlakeForm {
                    draft: draft,
                    error: add_error,
                    show_onboarding_callouts: from_setup(),
                    on_cancel: move |_| {
                        draft.set(NewFlakeDraft {
                            name: String::new(),
                            repo_url: String::new(),
                            branch: String::new(),
                            build_scope: "cf_systems_only".to_string(),
                            credential_type: "none".to_string(),
                            credential_username: String::new(),
                            credential_secret: String::new(),
                            credential_ssh_username: String::new(),
                        });
                        add_error.set(None);
                        show_add_form.set(false);
                    },
                    on_submit: move |_| {
                        let next = draft.read().clone();
                        if let Err(err) = validate_new_flake(&next, &flakes.read()) {
                            add_error.set(Some(err));
                            return;
                        }

                        let mut flakes = flakes.clone();
                        let mut draft = draft.clone();
                        let mut add_error = add_error.clone();
                        let mut show_add_form = show_add_form.clone();
                        let mut server_notice = server_notice.clone();
                        spawn(async move {
                            let request = CreateFlakeRequest {
                                name: next.name.trim().to_string(),
                                repo_url: next.repo_url.trim().to_string(),
                                branch: normalize_optional_branch(&next.branch),
                                build_scope: Some(next.build_scope.clone()),
                            };

                            match create_flake(&request).await {
                                Ok(created) => {
                                    if let Err(error) = save_flake_credentials(created.id, &next).await {
                                        add_error.set(Some(error));
                                        return;
                                    }
                                    let mut values = flakes.read().clone();
                                    values.push(FlakeListItem::from_registry(created));
                                    values
                                        .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                                    flakes.set(values);
                                    draft.set(NewFlakeDraft {
                                        name: String::new(),
                                        repo_url: String::new(),
                                        branch: String::new(),
                                        build_scope: "cf_systems_only".to_string(),
                                        credential_type: "none".to_string(),
                                        credential_username: String::new(),
                                        credential_secret: String::new(),
                                        credential_ssh_username: String::new(),
                                    });
                                    add_error.set(None);
                                    server_notice.set(None);
                                    show_add_form.set(false);
                                }
                                Err(error) => {
                                    add_error.set(Some(error.to_string()));
                                }
                            }
                        });
                    },
                }
            }

            FiltersBar {
                environments: environments.clone(),
                search: search,
                environment_filter: environment_filter,
                commit_filter: commit_filter,
                size_filter: size_filter,
                open_dropdown: open_dropdown,
            }

            if *loading_flakes.read() {
                Card {
                    title: Some("Loading flakes".to_string()),
                    children: rsx! {
                        p { class: "{theme::text::SECONDARY}", "Fetching registry entries from API..." }
                    }
                }
            } else if filtered_flakes.is_empty() {
                Card {
                    title: Some("No flakes".to_string()),
                    children: rsx! {
                        p { class: "{theme::text::SECONDARY}", "No flakes matched your filters." }
                    }
                }
            } else if *view_mode.read() == FlakesViewMode::Cards {
                div {
                    class: "grid grid-cols-1 xl:grid-cols-2 gap-6",
                    "data-testid": "flakes-cards",
                    for flake in filtered_flakes.clone() {
                        FlakeCard {
                            flake,
                            selected_history_flake_id: *selected_history_flake.read(),
                            on_select_history_flake: move |id| {
                                selected_history_flake.set(Some(id));
                                selected_history_commit.set(None);
                            },
                            on_remove: move |id| remove_flake_by_id(flakes, pending_remove, id),
                            on_edit: move |id| start_edit_flake(flakes, editing_flake, edit_error, id),
                            on_refresh: move |id| refresh_flake_by_id(id, refreshing_flake, sync_note),
                        }
                    }
                }
            } else {
                FlakesTable {
                    flakes: filtered_flakes.clone(),
                    selected_history_flake_id: *selected_history_flake.read(),
                    on_select_history_flake: move |id| {
                        selected_history_flake.set(Some(id));
                        selected_history_commit.set(None);
                    },
                    on_remove: move |id| remove_flake_by_id(flakes, pending_remove, id),
                    on_edit: move |id| start_edit_flake(flakes, editing_flake, edit_error, id),
                    on_refresh: move |id| refresh_flake_by_id(id, refreshing_flake, sync_note),
                }
            }

            FlakeHistoryExplorer {
                flakes: filtered_flakes.clone(),
                selected_flake_id: selected_history_flake,
                selected_commit_hash: selected_history_commit,
                timelines: flake_timelines.read().clone(),
                cve_scan_status: cve_scan_status,
            }

            if let Some(editing) = editing_flake.read().clone() {
                EditFlakeDialog {
                    draft: editing,
                    error: edit_error,
                    environments: db_environments.clone(),
                    on_remove: move |flake_id| {
                        let target = flakes
                            .read()
                            .iter()
                            .find(|item| item.id == flake_id)
                            .cloned();
                        if let Some(flake) = target {
                            pending_remove.set(Some(flake));
                            editing_flake.set(None);
                            edit_error.set(None);
                        }
                    },
                    on_cancel: move |_| {
                        editing_flake.set(None);
                        edit_error.set(None);
                    },
                    on_change: move |next| editing_flake.set(Some(next)),
                    on_submit: move |_| {
                        let Some(next) = editing_flake.read().clone() else {
                            return;
                        };
                        if let Err(err) = validate_flake_edit(&next, &flakes.read()) {
                            edit_error.set(Some(err));
                            return;
                        }

                        let mut flakes = flakes.clone();
                        let mut editing_flake = editing_flake.clone();
                        let mut edit_error = edit_error.clone();
                        let mut server_notice = server_notice.clone();
                        spawn(async move {
                            let request = UpdateFlakeRequest {
                                name: next.name.trim().to_string(),
                                repo_url: next.repo_url.trim().to_string(),
                                branch: normalize_optional_branch(&next.branch),
                                build_scope: Some(next.build_scope.clone()),
                            };

                            match update_flake(next.id, &request).await {
                                Ok(updated) => {
                                    if let Err(error) = save_flake_credentials(updated.id, &next).await {
                                        edit_error.set(Some(error));
                                        return;
                                    }
                                    let mut values = flakes.read().clone();
                                    if let Some(target) = values.iter_mut().find(|item| item.id == updated.id)
                                    {
                                        target.name = updated.name;
                                        target.repo_url = updated.repo_url;
                                        target.branch = updated.branch;
                                        target.build_scope = updated.build_scope;
                                        target.system_count = updated.system_count.max(0) as usize;
                                    }
                                    values.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                                    flakes.set(values);
                                    editing_flake.set(None);
                                    edit_error.set(None);
                                    server_notice.set(None);
                                }
                                Err(error) => {
                                    edit_error.set(Some(error.to_string()));
                                }
                            }
                        });
                    }
                }
            }

            if let Some(flake) = pending_remove.read().clone() {
                RemoveFlakeDialog {
                    flake_name: flake.name.clone(),
                    system_count: flake.system_count,
                    on_cancel: move |_| pending_remove.set(None),
                    on_confirm: move |(hard, cascade)| {
                        let mut flakes = flakes.clone();
                        let mut pending_remove = pending_remove.clone();
                        let mut server_notice = server_notice.clone();
                        let remove_id = flake.id;
                        spawn(async move {
                            match delete_flake(remove_id, hard, cascade).await {
                                Ok(()) => {
                                    let mut values = flakes.read().clone();
                                    values.retain(|item| item.id != remove_id);
                                    flakes.set(values);
                                    server_notice.set(None);
                                    pending_remove.set(None);
                                }
                                Err(error) => {
                                    server_notice.set(Some(error.to_string()));
                                }
                            }
                        });
                    }
                }
            }

            if let Some((flake_id, flake_name, detail)) = rewrite_prompt.read().clone() {
                HistoryRewriteDialog {
                    flake_name: flake_name.clone(),
                    detail,
                    on_cancel: move |_| rewrite_prompt.set(None),
                    on_accept: move |_| {
                        let flake_name_for_error = flake_name.clone();
                        let mut rewrite_prompt = rewrite_prompt.clone();
                        let mut sync_note = sync_note.clone();
                        let mut last_manual_sync = last_manual_sync.clone();
                        let mut flakes_signal = flakes.clone();
                        let mut timelines_signal = flake_timelines.clone();
                        spawn(async move {
                            match accept_flake_history_rewrite(flake_id).await {
                                Ok(response) => {
                                    rewrite_prompt.set(None);
                                    sync_note.set(Some(response.message));
                                    last_manual_sync.set(Some(Utc::now()));

                                    if let Ok(items) = fetch_flakes().await {
                                        flakes_signal.set(
                                            items
                                                .into_iter()
                                                .map(FlakeListItem::from_registry)
                                                .collect(),
                                        );
                                    }

                                    if let Ok(timelines) = fetch_flake_timelines().await {
                                        timelines_signal.set(timelines);
                                    }
                                }
                                Err(error) => {
                                    sync_note.set(Some(format!(
                                        "Failed to accept rewrite for {flake_name_for_error}: {error}"
                                    )));
                                }
                            }
                        });
                    },
                }
            }
        }
    }
}

#[component]
fn ViewToggle(view_mode: FlakesViewMode, on_change: EventHandler<FlakesViewMode>) -> Element {
    let table_active = view_mode == FlakesViewMode::Table;
    let cards_active = view_mode == FlakesViewMode::Cards;

    rsx! {
        div {
            class: "inline-flex rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG}",
            button {
                class: "px-3 py-2 text-sm font-medium rounded-l-lg transition {theme::interactive::FOCUS_RING} {theme::text::SECONDARY} {table_class(table_active)}",
                onclick: move |_| on_change.call(FlakesViewMode::Table),
                "Table"
            }
            button {
                class: "px-3 py-2 text-sm font-medium rounded-r-lg transition {theme::interactive::FOCUS_RING} {theme::text::SECONDARY} {table_class(cards_active)}",
                onclick: move |_| on_change.call(FlakesViewMode::Cards),
                "Cards"
            }
        }
    }
}

#[component]
fn FiltersBar(
    environments: Vec<String>,
    search: Signal<String>,
    environment_filter: Signal<Vec<String>>,
    commit_filter: Signal<Vec<CommitFilter>>,
    size_filter: Signal<Vec<SizeBucket>>,
    open_dropdown: Signal<Option<FilterDropdown>>,
) -> Element {
    rsx! {
        div {
            class: "relative z-[2000] grid grid-cols-1 lg:grid-cols-4 gap-4",
            style: "position: sticky; top: 0;",
            input {
                class: "rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                r#type: "search",
                placeholder: "Search flakes...",
                value: "{search.read()}",
                oninput: move |evt| search.set(evt.value()),
            }
            EnvironmentFilterDropdown {
                environments,
                selected: environment_filter,
                open_dropdown: open_dropdown,
            }
            CommitFilterDropdown {
                selected: commit_filter,
                open_dropdown: open_dropdown,
            }
            SizeFilterDropdown {
                selected: size_filter,
                open_dropdown: open_dropdown,
            }
        }
    }
}

#[component]
fn FlakesTable(
    flakes: Vec<FlakeListItem>,
    selected_history_flake_id: Option<i32>,
    on_select_history_flake: EventHandler<i32>,
    on_remove: EventHandler<i32>,
    on_edit: EventHandler<i32>,
    on_refresh: EventHandler<i32>,
) -> Element {
    let sort_column = use_signal(|| None::<SortColumn>);
    let sort_direction = use_signal(|| SortDirection::Asc);

    let sorted_flakes = {
        let mut sorted = flakes.clone();
        if let Some(column) = *sort_column.read() {
            let dir = *sort_direction.read();
            sorted.sort_by(|a, b| {
                let cmp = match column {
                    SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    SortColumn::Repo => a.repo_url.to_lowercase().cmp(&b.repo_url.to_lowercase()),
                    SortColumn::Systems => a.system_count.cmp(&b.system_count),
                    SortColumn::Environments => {
                        a.environments.join(", ").cmp(&b.environments.join(", "))
                    }
                    SortColumn::LatestCommit => {
                        let a_commit = a.latest_commit.as_deref().unwrap_or("");
                        let b_commit = b.latest_commit.as_deref().unwrap_or("");
                        a_commit.cmp(b_commit)
                    }
                };
                match dir {
                    SortDirection::Asc => cmp,
                    SortDirection::Desc => cmp.reverse(),
                }
            });
        }
        sorted
    };

    let current_col = *sort_column.read();
    let current_dir = *sort_direction.read();

    rsx! {
        div {
            class: "rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden shadow-sm bg-gray-900/60",
            div {
                class: "overflow-x-auto",
                "data-testid": "flakes-table",
                table {
                    class: "w-full",
                    thead {
                        class: "{theme::surface::SUBTLE_BG}",
                        tr {
                                SortableHeader {
                                    label: "Flake",
                                    column: SortColumn::Name,
                                    current_col: current_col,
                                    current_dir: current_dir,
                                    sort_column: sort_column,
                                    sort_direction: sort_direction,
                                }
                                SortableHeader {
                                    label: "Repository",
                                    column: SortColumn::Repo,
                                    current_col: current_col,
                                    current_dir: current_dir,
                                    sort_column: sort_column,
                                    sort_direction: sort_direction,
                                }
                                SortableHeader {
                                    label: "Systems",
                                    column: SortColumn::Systems,
                                    current_col: current_col,
                                    current_dir: current_dir,
                                    sort_column: sort_column,
                                    sort_direction: sort_direction,
                                }
                                SortableHeader {
                                    label: "Environments",
                                    column: SortColumn::Environments,
                                    current_col: current_col,
                                    current_dir: current_dir,
                                    sort_column: sort_column,
                                    sort_direction: sort_direction,
                                }
                                SortableHeader {
                                    label: "Latest Commit",
                                    column: SortColumn::LatestCommit,
                                    current_col: current_col,
                                    current_dir: current_dir,
                                    sort_column: sort_column,
                                    sort_direction: sort_direction,
                                }
                                th { class: "px-4 py-3 text-right text-xs font-medium text-gray-400 uppercase tracking-wider", "Actions" }
                            }
                        }
                    tbody {
                        class: "divide-y {theme::surface::DIVIDER}",
                        for flake in sorted_flakes {
                            {
                                let is_selected = selected_history_flake_id == Some(flake.id);
                                rsx! {
                                    tr {
                                        key: "{flake.id}",
                                        class: if is_selected {
                                            "cursor-pointer"
                                        } else {
                                            "hover:bg-gray-800/40 cursor-pointer"
                                        },
                                        style: if is_selected {
                                            "background-color: rgba(130, 105, 155, 0.32);"
                                        } else {
                                            "background-color: transparent;"
                                        },
                                        onclick: move |_| on_select_history_flake.call(flake.id),
                                        td { class: "{theme::spacing::TABLE_CELL} text-sm text-white", "{flake.name}" }
                                        td {
                                            class: "{theme::spacing::TABLE_CELL} text-sm text-gray-300",
                                            div {
                                                class: "space-y-1",
                                                p { class: "font-mono", "{flake.repo_url}" }
                                                p { class: "text-[11px] text-sky-300 font-mono", "branch: {flake.branch}" }
                                            }
                                        }
                                        td { class: "{theme::spacing::TABLE_CELL} text-sm text-gray-200", "{flake.system_count}" }
                                        td { class: "{theme::spacing::TABLE_CELL} text-sm {theme::text::SECONDARY}", "{environments_label(&flake)}" }
                                        td { class: "{theme::spacing::TABLE_CELL} text-sm text-gray-300 font-mono", "{latest_commit_label(&flake)}" }
                                        td {
                                            class: "{theme::spacing::TABLE_CELL} text-right",
                                            div {
                                                class: "inline-flex items-center gap-2",
                                                button {
                                                    class: "p-1.5 rounded-md border border-gray-700 text-gray-300 hover:text-white hover:border-gray-500 hover:bg-gray-800/70 transition-colors",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_edit.call(flake.id)
                                                    },
                                                    "aria-label": "Edit flake",
                                                    svg {
                                                        width: "14",
                                                        height: "14",
                                                        view_box: "0 0 24 24",
                                                        fill: "none",
                                                        stroke: "currentColor",
                                                        stroke_width: "2",
                                                        stroke_linecap: "round",
                                                        stroke_linejoin: "round",
                                                        circle { cx: "12", cy: "12", r: "3" }
                                                        path { d: "M19.4 15a1.7 1.7 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.82-.33 1.7 1.7 0 0 0-1 1.52V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1-1.52 1.7 1.7 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .33-1.82 1.7 1.7 0 0 0-1.52-1H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.52-1 1.7 1.7 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.82.33h.09a1.7 1.7 0 0 0 1-1.52V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.52 1.7 1.7 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.33 1.82v.09a1.7 1.7 0 0 0 1.52 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.52 1z" }
                                                    }
                                                }
                                                button {
                                                    class: "text-xs text-blue-400 hover:text-blue-300 px-2 py-1 rounded hover:bg-blue-500/10 transition-colors",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_refresh.call(flake.id)
                                                    },
                                                    "🔄"
                                                }
                                                button {
                                                    class: "text-xs text-red-400 hover:text-red-300 px-2 py-1 rounded hover:bg-red-500/10 transition-colors",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_remove.call(flake.id)
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
                }
            }
        }
    }
}

#[component]
fn FlakeCard(
    flake: FlakeListItem,
    selected_history_flake_id: Option<i32>,
    on_select_history_flake: EventHandler<i32>,
    on_remove: EventHandler<i32>,
    on_edit: EventHandler<i32>,
    on_refresh: EventHandler<i32>,
) -> Element {
    let latest_commit = latest_commit_label(&flake);
    let is_selected = selected_history_flake_id == Some(flake.id);

    rsx! {
        div {
            class: if is_selected {
                "rounded-xl border border-blue-400/70 overflow-hidden shadow-sm ring-2 ring-blue-400/40 cursor-pointer"
            } else {
                "rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden shadow-sm cursor-pointer"
            },
            onclick: move |_| on_select_history_flake.call(flake.id),
            div {
                class: "px-5 py-3 border-b border-gray-800 flex items-center justify-between",
                style: "background: linear-gradient(135deg, rgba(130, 105, 155, 0.42) 0%, rgba(17, 24, 39, 0.92) 100%);",
                div {
                    h3 { class: "text-lg font-semibold text-white", "{flake.name}" }
                    p { class: "text-xs text-gray-300 mt-1 font-mono", "{flake.repo_url}" }
                    p { class: "text-[11px] text-sky-300 mt-1 font-mono", "branch: {flake.branch}" }
                }
                button {
                    class: "p-1.5 rounded-md border border-gray-700 text-gray-300 hover:text-white hover:border-gray-500 hover:bg-gray-800/70 transition-colors",
                    onclick: move |evt| {
                        evt.stop_propagation();
                        on_edit.call(flake.id)
                    },
                    "aria-label": "Edit flake",
                    svg {
                        width: "14",
                        height: "14",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        circle { cx: "12", cy: "12", r: "3" }
                        path { d: "M19.4 15a1.7 1.7 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.82-.33 1.7 1.7 0 0 0-1 1.52V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1-1.52 1.7 1.7 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .33-1.82 1.7 1.7 0 0 0-1.52-1H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.52-1 1.7 1.7 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.82.33h.09a1.7 1.7 0 0 0 1-1.52V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.52 1.7 1.7 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.33 1.82v.09a1.7 1.7 0 0 0 1.52 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.52 1z" }
                    }
                }
            }
            div {
                class: "px-5 py-2 bg-gray-800/50",
                div {
                    class: "flex flex-wrap items-center gap-2 text-xs",
                    span {
                        class: "inline-flex px-2 py-1 rounded border text-gray-100 cf-chip-slate",
                        "{flake.system_count} systems"
                    }
                    span {
                        class: "inline-flex px-2 py-1 rounded border text-gray-100 cf-chip-teal",
                        "{flake.environments.len()} environments"
                    }
                }
            }
            div {
                class: "px-5 py-2 bg-gray-900 space-y-1.5",
                p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500", "Environments" }
                div {
                    class: "flex flex-wrap gap-1.5 max-h-12 overflow-hidden",
                    if flake.environments.is_empty() {
                        span { class: "text-xs text-gray-500", "None" }
                    } else {
                        for env in flake.environments.clone() {
                            span {
                                class: "inline-flex px-2 py-1 text-xs rounded border text-blue-100 cf-chip-blue",
                                "{env}"
                            }
                        }
                    }
                }
            }
            div {
                class: "px-5 py-2.5 bg-gray-800/50 flex items-center justify-between",
                div {
                    class: "space-y-1",
                    p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500", "Latest Commit" }
                    p { class: "text-sm text-gray-200 font-mono", "{latest_commit}" }
                }
                if flake.system_count > 0 {
                    div {
                        class: "inline-flex items-center gap-2",
                        span {
                            class: "text-xs text-gray-500",
                            "In Use"
                        }
                    }
                } else {
                    div {
                        class: "inline-flex items-center gap-2",
                        button {
                            class: "text-xs text-blue-400 hover:text-blue-300 px-2 py-1 rounded hover:bg-blue-500/10 transition-colors",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                on_refresh.call(flake.id)
                            },
                            "🔄 Refresh"
                        }
                        button {
                            class: "text-xs text-red-400 hover:text-red-300 px-2 py-1 rounded hover:bg-red-500/10 transition-colors",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                on_remove.call(flake.id)
                            },
                            "Remove"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FlakeHistoryExplorer(
    flakes: Vec<FlakeListItem>,
    selected_flake_id: Signal<Option<i32>>,
    selected_commit_hash: Signal<Option<String>>,
    timelines: Vec<FlakeTimeline>,
    cve_scan_status: Signal<HashMap<String, String>>,
) -> Element {
    use crate::hooks::websocket::{SystemEvalStatus, use_websocket_eval_stream};
    let navigator = use_navigator();

    let fallback_flake_id = flakes.first().map(|flake| flake.id).unwrap_or(0);
    let active_flake_id = (*selected_flake_id.read()).unwrap_or(fallback_flake_id);

    // Cache for loaded commit diffs
    let loaded_diffs = use_signal(|| HashMap::<(i32, String), String>::new());
    // Track current active commit hash to force re-render when diff loads
    let current_commit_key = use_signal(|| (0i32, String::new()));

    // Build only the active flake's commits for this render.
    // Keep this non-memoized so newly fetched timeline props are reflected immediately.
    let commits_vec = build_flake_commits(&timelines, active_flake_id);

    // Only stream eval updates after an explicit commit selection.
    // Auto-subscribing to the newest commit can flood the client on busy instances.
    let active_commit_for_ws = selected_commit_hash
        .read()
        .as_ref()
        .and_then(|hash| commits_vec.iter().find(|commit| &commit.hash == hash))
        .filter(|commit| commit.total_system_count <= MAX_WS_STREAM_SYSTEMS)
        .cloned();

    // Connect to WebSocket for active commit's eval status (MUST be unconditional hook call)
    let commit_id_str = active_commit_for_ws
        .as_ref()
        .map(|c| c.id.to_string())
        .unwrap_or_else(|| "0".to_string());
    let (_logs, system_status, _conn_state, _reconnect) = use_websocket_eval_stream(&commit_id_str);

    if flakes.is_empty() {
        return rsx! {
            Card {
                title: Some("Git Commit History".to_string()),
                children: rsx! {
                    p { class: "text-sm {theme::text::SECONDARY}", "No flakes in scope for commit history." }
                }
            }
        };
    }

    let active_flake = flakes
        .iter()
        .find(|flake| flake.id == active_flake_id)
        .cloned()
        .unwrap_or_else(|| flakes[0].clone());

    let active_commit = selected_commit_hash
        .read()
        .as_ref()
        .and_then(|hash| commits_vec.iter().find(|commit| &commit.hash == hash))
        .map(|commit| commit.clone())
        .or_else(|| commits_vec.first().cloned());

    // Load diff for the active commit if not already loaded
    // We read the signal INSIDE use_effect so it tracks the dependency
    {
        let loaded_diffs = loaded_diffs.clone();
        let current_key = current_commit_key.clone();

        use_effect(move || {
            // Read signals inside the effect so it re-runs when they change
            let selected_hash = selected_commit_hash.read().clone();
            let flake_id = active_flake.id;

            if let Some(commit_hash) = &selected_hash {
                let key = (flake_id, commit_hash.clone());
                let already_loaded = loaded_diffs.read().contains_key(&key);

                #[cfg(target_arch = "wasm32")]
                console::log_1(
                    &format!(
                        "Effect running - hash: {}, loaded: {}",
                        &commit_hash[..7.min(commit_hash.len())],
                        already_loaded
                    )
                    .into(),
                );

                if !already_loaded {
                    let commit_hash = commit_hash.clone();
                    let flake_id = flake_id;
                    let mut loaded_diffs_inner = loaded_diffs.clone();
                    let mut current_key_inner = current_key.clone();

                    #[cfg(target_arch = "wasm32")]
                    console::log_1(
                        &format!(
                            "Fetching diff for {}...",
                            &commit_hash[..7.min(commit_hash.len())]
                        )
                        .into(),
                    );

                    spawn(async move {
                        match fetch_commit_diff(flake_id, &commit_hash).await {
                            Ok(response) => {
                                #[cfg(target_arch = "wasm32")]
                                console::log_1(
                                    &format!("Diff loaded! {} bytes", response.diff.len()).into(),
                                );
                                loaded_diffs_inner
                                    .write()
                                    .insert(key.clone(), response.diff);
                                current_key_inner.set(key);
                            }
                            Err(e) => {
                                #[cfg(target_arch = "wasm32")]
                                console::log_1(&format!("Error: {}", e).into());
                                loaded_diffs_inner.write().insert(
                                    key.clone(),
                                    format!("Error loading diff: {}\n\nCommit: {}", e, commit_hash),
                                );
                                current_key_inner.set(key);
                            }
                        }
                    });
                }
            }
        });
    }

    // Update active_commit with loaded diff if available
    let active_commit = if let Some(commit) = active_commit.clone() {
        let key = (active_flake.id, commit.hash.clone());
        if let Some(diff) = loaded_diffs.read().get(&key) {
            let mut commit = commit;
            commit.diff = diff.clone();
            // Calculate stats from the diff
            let (files_changed, insertions, deletions) = diff_stats(&commit.diff);
            commit.files_changed = files_changed;
            commit.insertions = insertions;
            commit.deletions = deletions;
            Some(commit)
        } else {
            Some(commit)
        }
    } else {
        None
    };

    let active_repo = active_flake.repo_url.clone();
    let history_title = format!("Git Commit History - {}", active_flake.name);
    let flake_sync_label = active_flake
        .last_synced_at
        .format("%Y-%m-%d %H:%M UTC")
        .to_string();

    rsx! {
        Card {
            title: Some(history_title),
            children: rsx! {
                div {
                    class: "space-y-3",
                    div {
                        class: "text-xs {theme::text::MUTED}",
                        "{active_repo} · last sync {flake_sync_label}"
                    }
                    div {
                        class: "cf-flakes-history-split",
                        div {
                            class: "rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden cf-history-timeline-bg",
                            div {
                                class: "px-3 py-2 border-b {theme::surface::CARD_BORDER} text-xs uppercase tracking-wide text-gray-400",
                                "Timeline"
                            }
                            div {
                                class: "max-h-[68vh] overflow-y-auto",
                                if commits_vec.is_empty() {
                                    p { class: "p-4 text-sm {theme::text::SECONDARY}", "No commits available." }
                                } else {
                                    div {
                                        class: "relative px-2 py-2",
                                        div {
                                            class: "absolute bg-slate-700/80",
                                            style: "left: 14px; top: 0; bottom: 0; width: 2px;",
                                        }
                                        div {
                                            class: "space-y-3 relative",
                                            for commit in commits_vec.iter() {
                                                {
                                                    let commit_id_for_modal = commit.id;
                                                    let is_active = active_commit
                                                        .as_ref()
                                                        .map(|value| value.hash == commit.hash)
                                                        .unwrap_or(false);
                                                    let short_hash = commit.hash.chars().take(7).collect::<String>();
                                                    let commit_time = commit.committed_at.format("%b %d %H:%M").to_string();
                                                    let (message_title, message_secondary) = commit_message_lines(&commit.message, 120);
                                                    let commit_for_select = commit.hash.clone();
                                                    let commit_card_style = if is_active {
                                                        "background-color: #1B2940; border-color: #7C67A4;"
                                                    } else {
                                                        "background-color: #1A212E; border-color: #303A4A;"
                                                    };
                                                    let commit_node_style = if is_active {
                                                        "width: 20px; height: 20px; margin-top: 7px; border-color: #d9c9ea; background-color: #82699B; box-shadow: 0 0 10px 2px rgba(130, 105, 155, 0.45), inset 0 0 0 1px rgba(255,255,255,0.20);"
                                                    } else {
                                                        "width: 20px; height: 20px; margin-top: 7px; border-color: #475569; background-color: #0f172a;"
                                                    };
                                                    rsx! {
                                                        div {
                                                            key: "{commit.hash}",
                                                            class: "grid",
                                                            style: "grid-template-columns: 22px 10px minmax(0, 1fr); margin-left: -1px; align-items: start;",

                                                            div {
                                                                class: "rounded-full border-2 flex items-center justify-center",
                                                                style: "{commit_node_style}",
                                                                if is_active {
                                                                    svg {
                                                                        class: "w-2.5 h-2.5 text-white",
                                                                        fill: "none",
                                                                        stroke: "currentColor",
                                                                        stroke_width: "2.5",
                                                                        view_box: "0 0 24 24",
                                                                        path {
                                                                            stroke_linecap: "round",
                                                                            stroke_linejoin: "round",
                                                                            d: "M5 13l4 4L19 7"
                                                                        }
                                                                    }
                                                                }
                                                            }

                                                            div {
                                                                class: "relative",
                                                                style: "height: 20px; margin-top: 7px;",
                                                                div {
                                                                    style: if is_active {
                                                                        "position: absolute; top: 8px; left: 0; right: 0; height: 2px; border-radius: 2px; background-color: #82699B;"
                                                                    } else {
                                                                        "position: absolute; top: 8px; left: 0; right: 0; height: 2px; border-radius: 2px; background-color: #64748b;"
                                                                    },
                                                                }
                                                                div {
                                                                    style: if is_active {
                                                                        "position: absolute; top: 6px; right: -2px; width: 5px; height: 5px; transform: rotate(45deg); background-color: #82699B;"
                                                                    } else {
                                                                        "position: absolute; top: 6px; right: -2px; width: 5px; height: 5px; transform: rotate(45deg); background-color: #64748b;"
                                                                    },
                                                                }
                                                            }

                                                            button {
                                                                class: "w-full justify-self-start rounded-xl border px-4 py-3 text-left transition",
                                                                style: "{commit_card_style}",
                                                                onclick: move |_| selected_commit_hash.set(Some(commit_for_select.clone())),
                                                                p {
                                                                    class: "text-sm text-white font-semibold text-left leading-snug break-words",
                                                                    style: "text-align: left; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;",
                                                                    "{message_title}"
                                                                }
                                                                if let Some(secondary) = message_secondary {
                                                                    p {
                                                                        class: "mt-1 text-xs text-gray-300 leading-snug break-words",
                                                                        style: "display: -webkit-box; -webkit-line-clamp: 1; -webkit-box-orient: vertical; overflow: hidden;",
                                                                        "{secondary}"
                                                                    }
                                                                }
                                                                div {
                                                                    class: "mt-2 flex flex-wrap items-center gap-2 text-[10px]",
                                                                    span {
                                                                        class: "inline-flex items-center px-2.5 py-1 rounded border font-mono leading-none cf-chip-violet",
                                                                        "{short_hash}"
                                                                    }
                                                                    span {
                                                                        class: "inline-flex items-center px-2.5 py-1 rounded border text-gray-100 leading-none cf-chip-slate",
                                                                        "{commit.author}"
                                                                    }
                                                                    if commit.evaluation_error_message.is_some() {
                                                                        span {
                                                                            class: "px-1.5 py-0.5 rounded bg-red-500/30 text-red-300 text-[10px]",
                                                                            title: "This commit has evaluation errors",
                                                                            "❌ eval error"
                                                                        }
                                                                    }
                                                                    span {
                                                                        class: "text-[10px] text-gray-400",
                                                                        "{commit_time}"
                                                                    }
                                                                    button {
                                                                        class: "px-2 py-1 rounded border text-[10px] hover:opacity-80 transition-opacity cursor-pointer",
                                                                        style: "{eval_badge_style(commit.evaluation_status.as_deref())}",
                                                                        title: "Open Evaluations view",
                                                                        onclick: move |evt| {
                                                                            evt.stop_propagation();
                                                                            navigator.push(Route::EvaluationsCommitView { commit_id: commit_id_for_modal });
                                                                        },
                                                                        "eval: {eval_badge_label(commit.evaluation_status.as_deref())}"
                                                                    }
                                                                    if let Some(build_status) = commit.build_status.clone() {
                                                                        button {
                                                                            class: "px-2 py-1 rounded border text-[10px]",
                                                                            style: "{build_badge_style(&build_status)}",
                                                                            title: "Open Builds view",
                                                                            onclick: move |evt| {
                                                                                evt.stop_propagation();
                                                                                navigator.push(Route::BuildsView {});
                                                                            },
                                                                            "build: {build_badge_label(&build_status)}"
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
                                }
                            }
                        }

                        div {
                            class: "rounded-xl border {theme::surface::CARD_BORDER} bg-gray-900/50 overflow-hidden",
                            if let Some(commit) = active_commit {
                                {
                                    let (message_title, message_secondary) =
                                        commit_message_lines(&commit.message, 160);
                                    rsx! {
                                        div {
                                            class: "px-4 py-3 border-b {theme::surface::CARD_BORDER} space-y-2",
                                            p {
                                                class: "text-base text-white font-semibold text-left leading-snug whitespace-pre-wrap break-words",
                                                "{message_title}"
                                            }
                                            if let Some(secondary) = message_secondary {
                                                p {
                                                    class: "text-sm text-gray-300 text-left leading-snug whitespace-pre-wrap break-words",
                                                    "{secondary}"
                                                }
                                            }
                                            div {
                                                class: "flex flex-wrap items-center gap-2 text-xs",
                                                span {
                                                    class: "font-mono px-2.5 py-1 rounded border cf-chip-violet",
                                                    "{commit.hash}"
                                                }
                                                span { class: "px-2.5 py-1 rounded bg-gray-800 text-gray-300", "{commit.author}" }
                                                span {
                                                    class: "px-2 py-1 rounded bg-gray-800 text-gray-300",
                                                    {commit.committed_at.format("%Y-%m-%d %H:%M UTC").to_string()}
                                                }
                                                span { class: "px-2 py-1 rounded bg-blue-500/20 text-blue-200", "{commit.files_changed} files" }
                                                span { class: "px-2 py-1 rounded bg-emerald-500/20 text-emerald-200", "+{commit.insertions}" }
                                                span { class: "px-2 py-1 rounded bg-red-500/20 text-red-200", "-{commit.deletions}" }
                                                span {
                                                    class: "px-2 py-1 rounded border",
                                                    style: "{eval_badge_style(commit.evaluation_status.as_deref())}",
                                                    title: "Open Evaluations view",
                                                    onclick: move |_| {
                                                        navigator.push(Route::EvaluationsCommitView { commit_id: commit.id });
                                                    },
                                                    "eval: {eval_badge_label(commit.evaluation_status.as_deref())}"
                                                }
                                                if let Some(build_status) = commit.build_status.clone() {
                                                    button {
                                                        class: "px-2 py-1 rounded border",
                                                        style: "{build_badge_style(&build_status)}",
                                                        title: "Open Builds view",
                                                        onclick: move |_| {
                                                            navigator.push(Route::BuildsView {});
                                                        },
                                                        "build: {build_badge_label(&build_status)}"
                                                    }
                                                }
                                                span { class: "px-2 py-1 rounded bg-slate-700/70 text-slate-200", "{commit.total_system_count} configs" }
                                            }
                                            // Show evaluation error message if present
                                            if let Some(error_msg) = commit.evaluation_error_message.as_ref() {
                                                div {
                                                    class: "mt-3 px-3 py-2 rounded border border-red-500/50 bg-red-900/20",
                                                    p {
                                                        class: "text-xs font-semibold text-red-300 mb-1",
                                                        "❌ Evaluation Error"
                                                    }
                                                    pre {
                                                        class: "text-xs text-red-200 font-mono whitespace-pre-wrap break-words max-h-48 overflow-y-auto",
                                                        "{error_msg}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                div {
                                    class: "px-4 pb-4 space-y-2",
                                    p { class: "text-xs uppercase tracking-wide text-gray-400", "nixosConfigurations at this commit" }
                                    {
                                        let visible_configs: Vec<String> = if commit.systems.is_empty() {
                                            commit
                                                .system_paths
                                                .iter()
                                                .map(|detail| detail.config_name.clone())
                                                .take(MAX_SYSTEMS_STORED_PER_COMMIT)
                                                .collect()
                                        } else {
                                            commit.systems.clone()
                                        };
                                        rsx! {
                                    if visible_configs.is_empty() {
                                        p { class: "text-sm text-gray-500", "No nixosConfigurations discovered for this commit." }
                                    } else {
                                        div {
                                            class: "space-y-2",
                                            for (idx, config_name) in preview_systems(&visible_configs).iter().enumerate() {
                                                {
                                                    let status = system_status.read().get(config_name).cloned();
                                                    let chip_style = system_chip_style(status.as_ref());
                                                    let chip_class = if status == Some(SystemEvalStatus::Evaluating) {
                                                        "px-2 py-1 rounded border text-xs font-mono animate-pulse"
                                                    } else {
                                                        "px-2 py-1 rounded border text-xs font-mono"
                                                    };
                                                    let path_detail = commit
                                                        .system_paths
                                                        .iter()
                                                        .find(|path| path.config_name == *config_name || format!("{} [CF system]", path.config_name) == *config_name)
                                                        .cloned();
                                                    rsx! {
                                                        div {
                                                            key: "{idx}-{config_name}",
                                                            class: "rounded border border-slate-700/70 bg-slate-900/40 p-2 space-y-1",
                                                            div { class: "flex flex-wrap items-center gap-2",
                                                                span {
                                                                    class: "{chip_class}",
                                                                    style: "{chip_style}",
                                                                    "{truncate_system_label(config_name)}"
                                                                }
                                                                if let Some(detail) = path_detail.as_ref() {
                                                                    if detail.is_cf_system {
                                                                        span { class: "text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-200 border border-blue-500/40", "CF system" }
                                                                    }

                                                                    {
                                                                        let scan_key = format!("{}::{}", active_flake_id, detail.config_name);
                                                                        let current_scan_status = cve_scan_status.read().get(&scan_key).cloned();
                                                                        let disabled_reason = detail.cve_scan_blocked_reason.clone().unwrap_or_else(|| "CVE scan eligibility unavailable".to_string());
                                                                        let can_trigger_scan = detail.cve_scan_eligible;
                                                                        rsx! {
                                                                            button {
                                                                                class: "text-[10px] uppercase tracking-wide px-2 py-1 rounded border border-amber-500/50 bg-amber-500/10 text-amber-200 hover:bg-amber-500/20 disabled:opacity-60 disabled:cursor-not-allowed",
                                                                                disabled: !can_trigger_scan,
                                                                                title: if can_trigger_scan {
                                                                                    Some("Run CVE scan immediately")
                                                                                } else {
                                                                                    Some(disabled_reason.as_str())
                                                                                },
                                                                                onclick: {
                                                                                    let config_name = detail.config_name.clone();
                                                                                    move |_| {
                                                                                        if !can_trigger_scan {
                                                                                            return;
                                                                                        }

                                                                                        let key = format!("{}::{}", active_flake_id, config_name.clone());
                                                                                        let request_config_name = config_name.clone();
                                                                                        cve_scan_status.write().insert(key.clone(), "queued".to_string());

                                                                                        spawn(async move {
                                                                                            match trigger_flake_config_cve_scan(active_flake_id, &request_config_name).await {
                                                                                                Ok(triggered) => {
                                                                                                    cve_scan_status.write().insert(key.clone(), "running".to_string());
                                                                                                    for _ in 0..25 {
                                                                                                        match fetch_cve_scan_status(&triggered.scan_id).await {
                                                                                                            Ok(status) => {
                                                                                                                let normalized = status.status.to_lowercase();
                                                                                                                if normalized == "completed" {
                                                                                                                    cve_scan_status.write().insert(key.clone(), "completed".to_string());
                                                                                                                    break;
                                                                                                                }
                                                                                                                if normalized == "failed" {
                                                                                                                    cve_scan_status.write().insert(key.clone(), "failed".to_string());
                                                                                                                    break;
                                                                                                                }
                                                                                                                cve_scan_status.write().insert(key.clone(), "running".to_string());
                                                                                                            }
                                                                                                            Err(_) => {
                                                                                                                cve_scan_status.write().insert(key.clone(), "status_error".to_string());
                                                                                                                break;
                                                                                                            }
                                                                                                        }

                                                                                                        use gloo_timers::future::TimeoutFuture;
                                                                                                        TimeoutFuture::new(1500).await;
                                                                                                    }
                                                                                                }
                                                                                                Err(_) => {
                                                                                                    cve_scan_status.write().insert(key.clone(), "trigger_failed".to_string());
                                                                                                }
                                                                                            }
                                                                                        });
                                                                                    }
                                                                                },
                                                                                "Run CVE Scan"
                                                                            }
                                                                            if let Some(scan_state) = current_scan_status {
                                                                                span {
                                                                                    class: "text-[10px] px-1.5 py-0.5 rounded bg-slate-800 text-slate-200 border border-slate-600",
                                                                                    "scan: {scan_state}"
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            if let Some(detail) = path_detail {
                                                                p { class: "text-[11px] text-slate-300 break-all",
                                                                    span { class: "text-slate-500", "expected path: " }
                                                                    {detail.expected_store_path.unwrap_or_else(|| "unavailable".to_string())}
                                                                }
                                                                if detail.mapped_host_count > 1 {
                                                                    p { class: "text-[11px] text-blue-200",
                                                                        "{detail.mapped_host_count} mapped hosts; showing most recent host report."
                                                                    }
                                                                }
                                                                p { class: "text-[11px] text-slate-300 break-all",
                                                                    span { class: "text-slate-500", "current path" }
                                                                    if let Some(hostname) = detail.cf_hostname.as_ref() {
                                                                        span { class: "text-slate-500", " ({hostname}): " }
                                                                    } else {
                                                                        span { class: "text-slate-500", ": " }
                                                                    }
                                                                    {detail.current_store_path.unwrap_or_else(|| "not reported".to_string())}
                                                                }
                                                            } else {
                                                                p { class: "text-[11px] text-slate-400", "path details unavailable" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        if commit.total_system_count > visible_configs.len() {
                                            p {
                                                class: "text-xs text-amber-300",
                                                "Showing {visible_configs.len()} of {commit.total_system_count} configurations to keep the UI responsive."
                                            }
                                        } else if commit.total_system_count > MAX_SYSTEM_CHIPS_RENDER {
                                            p {
                                                class: "text-xs text-amber-300",
                                                "Showing first {MAX_SYSTEM_CHIPS_RENDER} of {commit.total_system_count} configurations to keep the UI responsive."
                                            }
                                        }
                                        p {
                                            class: "text-xs text-slate-400",
                                            "Expected path comes from commit derivation data; current path is host-scoped and shown for the selected mapped CF host (most recent report when multiple hosts share the config)."
                                        }
                                    }
                                        }
                                    }
                                }
                                div {
                                    class: "p-4 max-h-[68vh] overflow-auto",
                                    FriendlyDiffViewer {
                                        diff: commit.diff.clone(),
                                    }
                                }
                            } else {
                                p { class: "p-4 text-sm {theme::text::SECONDARY}", "Select a commit node to inspect the full diff." }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FriendlyDiffViewer(diff: String) -> Element {
    // Show loading message if diff is empty
    if diff.is_empty() {
        return rsx! {
            div {
                class: "text-sm text-gray-400 p-4 text-center",
                "Loading diff..."
            }
        };
    }

    let parsed_files = parse_unified_diff(&diff);
    if parsed_files.is_empty() {
        return rsx! {
            pre {
                class: "text-xs font-mono rounded-lg border border-gray-700 bg-gray-950 p-3 text-gray-200 overflow-x-auto",
                "{diff}"
            }
        };
    }

    let mut selected_file_index = use_signal(|| 0usize);
    let mut show_file_list = use_signal(|| true);
    let file_count = parsed_files.len();
    let is_file_list_open = *show_file_list.read();
    let active_index = (*selected_file_index.read()).min(file_count.saturating_sub(1));
    let active_file = parsed_files.get(active_index).cloned();
    let (total_insertions, total_deletions) = parsed_files
        .iter()
        .map(diff_file_stats)
        .fold((0usize, 0usize), |(ia, da), (ib, db)| (ia + ib, da + db));

    rsx! {
        div {
            class: "space-y-4",
            tabindex: "0",
            onkeydown: move |evt| {
                let key = evt.key();
                if file_count == 0 {
                    return;
                }
                if key == Key::ArrowDown {
                    evt.prevent_default();
                    let next = ((*selected_file_index.read()) + 1).min(file_count - 1);
                    selected_file_index.set(next);
                } else if key == Key::ArrowUp {
                    evt.prevent_default();
                    let next = (*selected_file_index.read()).saturating_sub(1);
                    selected_file_index.set(next);
                }
            },
            div {
                class: "rounded-lg border border-gray-700 bg-gray-900/70",
                div {
                    class: "px-3 py-2 border-b border-gray-700 flex items-center justify-between gap-3",
                    div {
                        class: "flex flex-wrap items-center gap-2 text-xs",
                        span { class: "px-2 py-1 rounded bg-blue-500/20 text-blue-200", "{file_count} files changed" }
                        span { class: "px-2 py-1 rounded bg-emerald-500/20 text-emerald-200", "+{total_insertions}" }
                        span { class: "px-2 py-1 rounded bg-red-500/20 text-red-200", "-{total_deletions}" }
                    }
                    button {
                        class: "text-xs px-2 py-1 rounded border border-gray-600 text-gray-300 hover:bg-gray-800",
                        onclick: move |_| show_file_list.set(!is_file_list_open),
                        if is_file_list_open { "Hide files" } else { "Show files" }
                    }
                }
                if is_file_list_open {
                    div {
                        class: "max-h-52 overflow-y-auto divide-y divide-gray-800",
                        for (idx, file) in parsed_files.iter().enumerate() {
                            {
                                let (insertions, deletions) = diff_file_stats(file);
                                let file_label = diff_file_label(file);
                                let is_active = idx == active_index;
                                rsx! {
                                    button {
                                        key: "{file.old_path}->{file.new_path}",
                                        class: if is_active {
                                            "w-full text-left px-3 py-2 border-l-2 border-violet-300"
                                        } else {
                                            "w-full text-left px-3 py-2 border-l-2 border-transparent hover:bg-gray-800/70"
                                        },
                                        style: if is_active {
                                            "background-color: rgba(130, 105, 155, 0.28);"
                                        } else {
                                            "background-color: transparent;"
                                        },
                                        onclick: move |_| selected_file_index.set(idx),
                                        div {
                                            class: "flex items-center justify-between gap-2",
                                            p { class: "text-xs font-mono text-gray-200 truncate", "{file_label}" }
                                            div {
                                                class: "flex items-center gap-2 text-[11px]",
                                                span { class: "text-emerald-300", "+{insertions}" }
                                                span { class: "text-red-300", "-{deletions}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(file) = active_file {
                {
                    let file_label = diff_file_label(&file);
                    let (insertions, deletions) = diff_file_stats(&file);
                    rsx! {
                        div {
                            class: "rounded-lg border border-gray-700 overflow-hidden",
                            div {
                                class: "px-3 py-2 border-b border-gray-700 bg-gray-900 flex items-center justify-between gap-2",
                                p { class: "text-xs font-mono text-gray-300 truncate", "{file_label}" }
                                div {
                                    class: "flex items-center gap-2",
                                    span { class: "text-[10px] uppercase tracking-wide text-gray-500", "{file.language}" }
                                    span { class: "text-[10px] text-emerald-300", "+{insertions}" }
                                    span { class: "text-[10px] text-red-300", "-{deletions}" }
                                }
                            }
                            div {
                                class: "bg-gray-950",
                                for line in file.lines {
                                    div {
                                        class: "grid",
                                        style: "grid-template-columns: 3.2rem 3.2rem 1.5rem minmax(0, 1fr);",
                                        class: "{line.class_name}",
                                        div { class: "px-2 py-0.5 text-[10px] text-gray-500 text-right border-r border-gray-800", "{line.old_number.map(|value| value.to_string()).unwrap_or_default()}" }
                                        div { class: "px-2 py-0.5 text-[10px] text-gray-500 text-right border-r border-gray-800", "{line.new_number.map(|value| value.to_string()).unwrap_or_default()}" }
                                        div { class: "px-1 py-0.5 text-[11px] text-gray-400 border-r border-gray-800", "{line.prefix}" }
                                        div {
                                            class: if line.is_hunk_header {
                                                "px-2 py-0.5 text-[11px] font-mono text-sky-300"
                                            } else {
                                                "px-2 py-0.5 text-[11px] font-mono text-gray-200 hljs language-{file.language}"
                                            },
                                            if line.is_hunk_header {
                                                "{line.content}"
                                            } else {
                                                span { dangerous_inner_html: "{highlight_diff_fragment(file.language, &line.content)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                p { class: "text-sm text-gray-400", "No file diff selected." }
            }
        }
    }
}

#[component]
fn AddFlakeForm(
    draft: Signal<NewFlakeDraft>,
    error: Signal<Option<String>>,
    show_onboarding_callouts: bool,
    on_submit: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut show_name_callout = use_signal(|| show_onboarding_callouts);
    let mut show_repo_callout = use_signal(|| show_onboarding_callouts);
    let mut show_branch_callout = use_signal(|| show_onboarding_callouts);

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
            style: "z-index: 3300;",
            onclick: move |_| on_cancel.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 cf-modal-panel-44",
                onclick: |evt| evt.stop_propagation(),
                h3 { class: "text-lg font-semibold text-white mb-1", "Register Flake" }
                div {
                    class: "space-y-4",
                    p {
                        class: "text-sm {theme::text::SECONDARY}",
                        "Schema context: {FLAKE_TABLE_SCHEMA_NOTE}."
                    }
                    div {
                        class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                        label {
                            class: "relative block space-y-2 overflow-visible",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Flake Name" }
                            input {
                                class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{draft.read().name}",
                                placeholder: "prod-core",
                                onfocus: move |_| show_name_callout.set(false),
                                oninput: move |evt| {
                                    let mut next = draft.read().clone();
                                    next.name = evt.value();
                                    draft.set(next);
                                    show_name_callout.set(false);
                                },
                            }
                            if show_name_callout() && draft.read().name.trim().is_empty() {
                                div {
                                    "data-testid": "setup-coach-flake-field-name",
                                    style: "position:absolute; left:0; top:calc(100% + 8px); width:min(340px, 92vw); z-index:70; background:rgba(30,64,175,0.94); border:1px solid rgba(96,165,250,0.75); border-radius:10px; padding:8px 10px; color:#dbeafe; font-size:12px; box-shadow:0 10px 24px rgba(15,23,42,0.45);",
                                    div {
                                        style: "position:absolute; top:-6px; left:18px; width:10px; height:10px; background:rgba(30,64,175,0.94); border-left:1px solid rgba(96,165,250,0.75); border-top:1px solid rgba(96,165,250,0.75); transform:rotate(45deg);"
                                    }
                                    p { style: "margin:0; color:#eff6ff; font-weight:600;", "Next action" }
                                    p { style: "margin:2px 0 0 0;", "Use a stable name admins will recognize (for example: prod-core or edge-fleet)." }
                                }
                            }
                        }
                        label {
                            class: "relative block space-y-2 overflow-visible",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Repository URL" }
                            input {
                                class: "w-full rounded-lg px-3 py-2 text-sm font-mono {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{draft.read().repo_url}",
                                placeholder: "https://github.com/org/repo",
                                onfocus: move |_| show_repo_callout.set(false),
                                oninput: move |evt| {
                                    let mut next = draft.read().clone();
                                    next.repo_url = evt.value();
                                    draft.set(next);
                                    show_repo_callout.set(false);
                                },
                            }
                            if show_repo_callout()
                                && !draft.read().name.trim().is_empty()
                                && draft.read().repo_url.trim().is_empty()
                            {
                                div {
                                    "data-testid": "setup-coach-flake-field-repo",
                                    style: "position:absolute; left:0; top:calc(100% + 8px); width:min(340px, 92vw); z-index:70; background:rgba(30,64,175,0.94); border:1px solid rgba(96,165,250,0.75); border-radius:10px; padding:8px 10px; color:#dbeafe; font-size:12px; box-shadow:0 10px 24px rgba(15,23,42,0.45);",
                                    div {
                                        style: "position:absolute; top:-6px; left:18px; width:10px; height:10px; background:rgba(30,64,175,0.94); border-left:1px solid rgba(96,165,250,0.75); border-top:1px solid rgba(96,165,250,0.75); transform:rotate(45deg);"
                                    }
                                    p { style: "margin:0; color:#eff6ff; font-weight:600;", "Next action" }
                                    p { style: "margin:2px 0 0 0;", "Point to the Git repository that contains your flake outputs for systems to deploy." }
                                }
                            }
                        }
                        label {
                            class: "relative block space-y-2 overflow-visible",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Branch (optional)" }
                            input {
                                class: "w-full rounded-lg px-3 py-2 text-sm font-mono {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{draft.read().branch}",
                                placeholder: "main",
                                onfocus: move |_| show_branch_callout.set(false),
                                oninput: move |evt| {
                                    let mut next = draft.read().clone();
                                    next.branch = evt.value();
                                    draft.set(next);
                                    show_branch_callout.set(false);
                                },
                            }
                            if show_branch_callout()
                                && !draft.read().name.trim().is_empty()
                                && !draft.read().repo_url.trim().is_empty()
                                && draft.read().branch.trim().is_empty()
                            {
                                div {
                                    "data-testid": "setup-coach-flake-field-branch",
                                    style: "position:absolute; left:0; top:calc(100% + 8px); width:min(340px, 92vw); z-index:70; background:rgba(30,64,175,0.94); border:1px solid rgba(96,165,250,0.75); border-radius:10px; padding:8px 10px; color:#dbeafe; font-size:12px; box-shadow:0 10px 24px rgba(15,23,42,0.45);",
                                    div {
                                        style: "position:absolute; top:-6px; left:18px; width:10px; height:10px; background:rgba(30,64,175,0.94); border-left:1px solid rgba(96,165,250,0.75); border-top:1px solid rgba(96,165,250,0.75); transform:rotate(45deg);"
                                    }
                                    p { style: "margin:0; color:#eff6ff; font-weight:600;", "Next action" }
                                    p { style: "margin:2px 0 0 0;", "Optional: pick a branch if deployments should track something other than the repo default." }
                                }
                            }
                        }
                        label {
                            class: "space-y-2 block md:col-span-2",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Build Scope" }
                            select {
                                class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{draft.read().build_scope}",
                                onchange: move |evt| {
                                    let mut next = draft.read().clone();
                                    next.build_scope = evt.value();
                                    draft.set(next);
                                },
                                option { value: "cf_systems_only", "Only Crystal Forge systems" }
                                option { value: "all_configs", "All nixosConfigurations in flake" }
                            }
                            p {
                                class: "text-xs {theme::text::SECONDARY}",
                                "Choose whether Crystal Forge should only build configurations mapped to managed systems or every configuration exported by the flake."
                            }
                        }
                        FlakeCredentialFields {
                            flake_id: None,
                            repo_url: draft.read().repo_url.clone(),
                            branch: draft.read().branch.clone(),
                            credential_type: draft.read().credential_type.clone(),
                            credential_username: draft.read().credential_username.clone(),
                            credential_secret: draft.read().credential_secret.clone(),
                            credential_ssh_username: draft.read().credential_ssh_username.clone(),
                            has_existing_secret: false,
                            on_change: move |(field, value): (String, String)| {
                                let mut next = draft.read().clone();
                                match field.as_str() {
                                    "credential_type" => next.credential_type = value,
                                    "credential_username" => next.credential_username = value,
                                    "credential_secret" => next.credential_secret = value,
                                    "credential_ssh_username" => next.credential_ssh_username = value,
                                    _ => {}
                                }
                                draft.set(next);
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
                            onclick: move |_| on_cancel.call(()),
                            "Cancel"
                        }
                        button {
                            class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                            onclick: move |_| on_submit.call(()),
                            "Save Flake"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RemoveFlakeDialog(
    flake_name: String,
    system_count: usize,
    on_confirm: EventHandler<(bool, bool)>, // (hard_delete, cascade)
    on_cancel: EventHandler<()>,
) -> Element {
    let mut hard_delete = use_signal(|| false);
    let mut cascade = use_signal(|| false);
    let mut confirm_text = use_signal(|| String::new());
    let mut deleting = use_signal(|| false);

    let has_dependencies = system_count > 0;
    let needs_cascade = has_dependencies && !cascade();
    let needs_hard_confirm = hard_delete();
    let can_proceed = if needs_hard_confirm {
        confirm_text() == "DELETE"
    } else {
        true
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
            onclick: move |_| {
                if !deleting() {
                    on_cancel.call(())
                }
            },
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 cf-modal-panel-34",
                onclick: |evt| evt.stop_propagation(),

                // Header
                h3 {
                    class: "text-lg font-semibold text-white mb-2",
                    "Delete flake {flake_name}?"
                }

                // Warning message
                if has_dependencies {
                    div {
                        class: "mb-4 p-3 rounded-lg bg-amber-500/10 border border-amber-500/30",
                        div {
                            class: "flex items-start gap-2",
                            svg {
                                class: "w-5 h-5 text-amber-400 shrink-0 mt-0.5",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                                }
                            }
                            div {
                                p {
                                    class: "text-sm font-medium text-amber-200",
                                    "This flake is linked to {system_count} system(s)"
                                }
                                p {
                                    class: "text-xs text-amber-300/80 mt-1",
                                    "Enable cascade delete to remove all related evaluations, builds, and deployments"
                                }
                            }
                        }
                    }
                } else {
                    p {
                        class: "text-sm {theme::text::SECONDARY} mb-4",
                        "This will soft-delete the flake (can be recovered). Related commits are retained."
                    }
                }

                // Delete options
                div {
                    class: "space-y-3 mb-6",

                    // Cascade checkbox (only if has dependencies)
                    if has_dependencies {
                        label {
                            class: "flex items-start gap-3 cursor-pointer",
                            input {
                                r#type: "checkbox",
                                class: "mt-1 w-4 h-4 rounded border-gray-600 bg-gray-800 text-violet-500 focus:ring-violet-500 focus:ring-offset-gray-900",
                                checked: cascade(),
                                onchange: move |evt| cascade.set(evt.checked())
                            }
                            div {
                                span {
                                    class: "text-sm font-medium text-gray-200",
                                    "Also delete all evaluations, builds, and deployments (cascade)"
                                }
                                p {
                                    class: "text-xs text-gray-400 mt-0.5",
                                    "This will permanently remove all related data"
                                }
                            }
                        }
                    }

                    // Hard delete checkbox
                    label {
                        class: "flex items-start gap-3 cursor-pointer",
                        input {
                            r#type: "checkbox",
                            class: "mt-1 w-4 h-4 rounded border-gray-600 bg-gray-800 text-red-500 focus:ring-red-500 focus:ring-offset-gray-900",
                            checked: hard_delete(),
                            onchange: move |evt| {
                                hard_delete.set(evt.checked());
                                if !evt.checked() {
                                    confirm_text.set(String::new());
                                }
                            }
                        }
                        div {
                            span {
                                class: "text-sm font-medium text-gray-200",
                                "Permanently delete (hard delete)"
                            }
                            p {
                                class: "text-xs text-gray-400 mt-0.5",
                                "Cannot be undone. Default is soft delete (recoverable)"
                            }
                        }
                    }
                }

                // Hard delete confirmation input
                if hard_delete() {
                    div {
                        class: "mb-6 p-4 rounded-lg bg-red-500/10 border border-red-500/30",
                        p {
                            class: "text-sm font-medium text-red-200 mb-2",
                            "⚠️ This will PERMANENTLY delete \"{flake_name}\" and cannot be undone."
                        }
                        p {
                            class: "text-xs text-red-300/80 mb-3",
                            "Type DELETE to confirm:"
                        }
                        input {
                            class: "w-full rounded-lg px-3 py-2 text-sm font-mono bg-gray-800 border border-gray-600 focus:border-red-500 focus:ring-1 focus:ring-red-500 text-white",
                            r#type: "text",
                            placeholder: "DELETE",
                            value: "{confirm_text()}",
                            oninput: move |evt| confirm_text.set(evt.value())
                        }
                    }
                }

                // Buttons
                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white",
                        disabled: deleting(),
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors",
                        class: if can_proceed && !needs_cascade && !deleting() {
                            "bg-red-500 hover:bg-red-400 text-white"
                        } else {
                            "bg-gray-600 text-gray-400 cursor-not-allowed"
                        },
                        disabled: !can_proceed || needs_cascade || deleting(),
                        onclick: move |_| {
                            deleting.set(true);
                            on_confirm.call((hard_delete(), cascade()));
                        },
                        if deleting() {
                            "Deleting..."
                        } else if needs_cascade {
                            "Enable cascade to proceed"
                        } else if needs_hard_confirm && !can_proceed {
                            "Type DELETE to confirm"
                        } else {
                            "Delete Flake"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HistoryRewriteDialog(
    flake_name: String,
    detail: String,
    on_cancel: EventHandler<MouseEvent>,
    on_accept: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
            style: "z-index: 3400;",
            div {
                class: "relative {theme::surface::CARD_BG} rounded-xl border {theme::surface::CARD_BORDER} shadow-2xl p-6 cf-modal-panel-34 max-w-2xl w-full",
                h3 {
                    class: "text-lg font-semibold {theme::text::PRIMARY}",
                    "History Rewrite Detected"
                }
                p {
                    class: "mt-2 text-sm {theme::text::SECONDARY}",
                    "{flake_name} has diverged from stored commit lineage."
                }
                p {
                    class: "mt-2 text-sm {theme::text::SECONDARY}",
                    "Accepting rewrite will clear this flake's stored commit history and resync from current branch HEAD."
                }
                div {
                    class: "mt-3 rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-xs text-amber-200 font-mono break-words",
                    "{detail}"
                }
                div {
                    class: "mt-6 flex items-center justify-end gap-3",
                    button {
                        class: "px-3 py-2 rounded-lg text-sm font-medium {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                        onclick: move |evt| on_cancel.call(evt),
                        "Cancel"
                    }
                    button {
                        class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING}",
                        onclick: move |evt| on_accept.call(evt),
                        "Accept rewrite and resync"
                    }
                }
            }
        }
    }
}

#[component]
fn EditFlakeDialog(
    draft: EditFlakeDraft,
    error: Signal<Option<String>>,
    environments: Vec<EnvironmentSummary>,
    on_change: EventHandler<EditFlakeDraft>,
    on_submit: EventHandler<()>,
    on_remove: EventHandler<i32>,
    on_cancel: EventHandler<()>,
) -> Element {
    let draft_for_name = draft.clone();
    let draft_for_repo = draft.clone();
    let draft_for_branch = draft.clone();
    let draft_for_description = draft.clone();
    let draft_for_build_scope = draft.clone();
    let draft_signal = use_signal(|| draft.clone());
    {
        let mut draft_signal = draft_signal.clone();
        let draft = draft.clone();
        use_effect(move || {
            draft_signal.set(draft.clone());
        });
    }

    rsx! {
        div {
            class: "modal-backdrop",
            style: "z-index: 3200;",
            onclick: move |_| on_cancel.call(()),
            div {
                class: "modal",
                style: "width: min(620px, 96vw); max-height: 92vh;",
                onclick: |evt| evt.stop_propagation(),
                div { class: "modal-head",
                    h2 {
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            style: "margin-right: 6px; vertical-align: text-bottom;",
                            circle { cx: "12", cy: "12", r: "3" }
                            path { d: "M19.4 15a1.7 1.7 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.82-.33 1.7 1.7 0 0 0-1 1.52V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1-1.52 1.7 1.7 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .33-1.82 1.7 1.7 0 0 0-1.52-1H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.52-1 1.7 1.7 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.82.33h.09a1.7 1.7 0 0 0 1-1.52V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.52 1.7 1.7 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.33 1.82v.09a1.7 1.7 0 0 0 1.52 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.52 1z" }
                        }
                        "Edit {draft.name}"
                    }
                    p {
                        "Update flake registration. URL changes will trigger a re-clone."
                    }
                }

                div {
                    class: "modal-body",
                    style: "overflow-y: auto;",
                    label {
                        class: "field",
                        span { "Name" }
                        input {
                            class: "input focus-ring",
                            value: "{draft.name}",
                            placeholder: "e.g. infrastructure",
                            oninput: move |evt| {
                                let mut next = draft_for_name.clone();
                                next.name = evt.value();
                                on_change.call(next);
                            }
                        }
                    }
                    label {
                        class: "field",
                        span { "Repository URL" }
                        input {
                            class: "input focus-ring mono",
                            value: "{draft.repo_url}",
                            placeholder: "git+ssh://git@gitlab.example.com/…",
                            style: "font-size: 12px;",
                            oninput: move |evt| {
                                let mut next = draft_for_repo.clone();
                                next.repo_url = evt.value();
                                on_change.call(next);
                            }
                        }
                    }
                    div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 14px;",
                        label {
                            class: "field",
                            span { "Branch" }
                            input {
                                class: "input focus-ring",
                                value: "{draft.branch}",
                                oninput: move |evt| {
                                    let mut next = draft_for_branch.clone();
                                    next.branch = evt.value();
                                    on_change.call(next);
                                }
                            }
                        }
                        label {
                            class: "field",
                            span { "Build Scope" }
                            select {
                                class: "input focus-ring",
                                value: "{draft.build_scope}",
                                onchange: move |evt| {
                                    let mut next = draft_for_build_scope.clone();
                                    next.build_scope = evt.value();
                                    on_change.call(next);
                                },
                                option { value: "cf_systems_only", "CF systems only" }
                                option { value: "all_configs", "All nixosConfigurations" }
                            }
                        }
                    }

                    div { class: "field",
                        label { "Description" }
                        input {
                            class: "input focus-ring",
                            value: "{draft.description}",
                            placeholder: "Short description shown in the registry",
                            oninput: move |evt| {
                                let mut next = draft_for_description.clone();
                                next.description = evt.value();
                                on_change.call(next);
                            },
                        }
                    }

                    FlakeCredentialFields {
                        flake_id: Some(draft.id),
                        repo_url: draft.repo_url.clone(),
                        branch: draft.branch.clone(),
                        credential_type: draft.credential_type.clone(),
                        credential_username: draft.credential_username.clone(),
                        credential_secret: draft.credential_secret.clone(),
                        credential_ssh_username: draft.credential_ssh_username.clone(),
                        has_existing_secret: draft.has_existing_secret,
                        on_change: move |(field, value): (String, String)| {
                            let mut draft_signal = draft_signal.clone();
                            let mut next = draft_signal.read().clone();
                            match field.as_str() {
                                "credential_type" => next.credential_type = value,
                                "credential_username" => next.credential_username = value,
                                "credential_secret" => next.credential_secret = value,
                                "credential_ssh_username" => next.credential_ssh_username = value,
                                _ => {}
                            }
                            draft_signal.set(next.clone());
                            on_change.call(next);
                        }
                    }

                    div { style: "margin-top: 10px; padding-top: 14px; border-top: 1px solid var(--cf-divider);",
                        div {
                            style: "font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.08em; color: var(--cf-text-muted); margin-bottom: 8px;",
                            "Danger zone"
                        }
                        button {
                            class: "btn btn-ghost focus-ring",
                            style: "color: #f87171; border-color: rgba(248,113,113,0.3);",
                            onclick: move |_| on_remove.call(draft.id),
                            svg {
                                width: "12",
                                height: "12",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                style: "margin-right: 6px; vertical-align: text-bottom;",
                                path { d: "M18 6 6 18M6 6l12 12" }
                            }
                            "Remove flake from registry"
                        }
                    }
                    if let Some(message) = error.read().clone() {
                        p { class: "text-sm text-red-300", "{message}" }
                    }
                }

                div {
                    class: "modal-foot",
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        onclick: move |_| on_submit.call(()),
                        "Save changes"
                    }
                }
            }
        }
    }
}

#[component]
fn FlakeCredentialFields(
    flake_id: Option<i32>,
    repo_url: String,
    branch: String,
    credential_type: String,
    credential_username: String,
    credential_secret: String,
    credential_ssh_username: String,
    has_existing_secret: bool,
    on_change: EventHandler<(String, String)>,
) -> Element {
    let mut test_state = use_signal(|| None::<String>);
    let is_no_credentials = credential_type == "none";
    let can_test = !is_no_credentials && flake_id.is_some();
    let credential_type_for_test = credential_type.clone();
    let credential_username_for_test = credential_username.clone();
    let credential_secret_for_test = credential_secret.clone();
    let credential_ssh_username_for_test = credential_ssh_username.clone();
    let on_change_none = on_change.clone();
    let on_change_ssh = on_change.clone();
    let on_change_pat = on_change.clone();
    rsx! {
        div {
            style: "margin-top: 8px; padding: 14px; border: 1px solid var(--cf-divider); border-radius: 10px; background: color-mix(in oklab, var(--cf-page-bg) 50%, var(--cf-card-bg));",
            div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;",
                div { style: "font-size: 13px; font-weight: 600; display: flex; align-items: center; gap: 6px;",
                    svg {
                        width: "13",
                        height: "13",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M21 2H3a1 1 0 0 0-1 1v4a1 1 0 0 0 1 1h18a1 1 0 0 0 1-1V3a1 1 0 0 0-1-1Z" }
                        path { d: "M10 16H3a1 1 0 0 0-1 1v4a1 1 0 0 0 1 1h7a1 1 0 0 0 1-1v-4a1 1 0 0 0-1-1Z" }
                        path { d: "M21 16h-3" }
                        path { d: "M16 20h6" }
                        path { d: "M19 16v4" }
                    }
                    "Repository credentials"
                }
                button {
                    class: "btn btn-ghost focus-ring xs",
                    onclick: move |_| {
                        if !can_test {
                            return;
                        }
                        test_state.set(Some("testing".to_string()));
                        let Some(flake_id) = flake_id else {
                            return;
                        };
                        let repo_url = repo_url.clone();
                        let branch = branch.clone();
                        let auth_type = credential_type_for_test.clone();
                        let username = credential_username_for_test.clone();
                        let secret = credential_secret_for_test.clone();
                        let ssh_username = credential_ssh_username_for_test.clone();
                        let mut test_state = test_state.clone();
                        spawn(async move {
                            let request = TestFlakeCredentialRequest {
                                repo_url: Some(repo_url),
                                branch: Some(branch),
                                auth_type,
                                username: normalize_optional_value(&username),
                                secret: normalize_optional_value(&secret),
                                ssh_username: normalize_optional_value(&ssh_username),
                                use_stored_secret_if_empty: true,
                            };

                            match test_flake_credentials(flake_id, &request).await {
                                Ok(response) => test_state.set(Some(format!("ok:{}", response.message))),
                                Err(err) => test_state.set(Some(format!("error:{}", err))),
                            }
                        });
                    },
                    disabled: !can_test || test_state.read().as_deref() == Some("testing"),
                    if is_no_credentials {
                        "Test connection"
                    } else if flake_id.is_none() {
                        "Save flake first"
                    } else if test_state.read().as_deref() == Some("testing") {
                        "Testing..."
                    } else {
                        "Test connection"
                    }
                }
            }

            if let Some(result) = test_state.read().clone() {
                div {
                    style: if result.starts_with("ok:") {
                        "margin-bottom: 10px; font-size: 11px; color: #34d399;"
                    } else if result.starts_with("error:") {
                        "margin-bottom: 10px; font-size: 11px; color: #f87171;"
                    } else {
                        "margin-bottom: 10px; font-size: 11px; color: var(--cf-text-muted);"
                    },
                    if let Some(msg) = result.strip_prefix("ok:") {
                        "{msg}"
                    } else if let Some(msg) = result.strip_prefix("error:") {
                        "{msg}"
                    }
                }
            }

            div { class: "seg", style: "margin-bottom: 12px;",
                button {
                    class: if credential_type == "none" { "active" } else { "" },
                    onclick: move |_| on_change_none.call(("credential_type".to_string(), "none".to_string())),
                    "None (public)"
                }
                button {
                    class: if credential_type == "ssh_key" { "active" } else { "" },
                    onclick: move |_| on_change_ssh.call(("credential_type".to_string(), "ssh_key".to_string())),
                    "SSH key"
                }
                button {
                    class: if credential_type == "pat" || credential_type == "username_password" { "active" } else { "" },
                    onclick: move |_| on_change_pat.call(("credential_type".to_string(), "pat".to_string())),
                    "HTTPS token"
                }
            }

            if credential_type == "ssh_key" {
                div {
                    style: "display: grid; gap: 10px;",
                    div { style: "display: grid; grid-template-columns: auto 1fr; gap: 10px; align-items: center;",
                        span { style: "font-size: 13px; color: var(--cf-text-secondary);", "SSH username" }
                        input {
                            class: "input focus-ring",
                            value: "{credential_ssh_username}",
                            placeholder: "git",
                            oninput: move |evt| on_change.call(("credential_ssh_username".to_string(), evt.value())),
                        }
                    }
                    div {
                        class: "field",
                        span { "Private key" }
                        textarea {
                            class: "input focus-ring",
                            rows: "6",
                            value: "{credential_secret}",
                            placeholder: if has_existing_secret {
                                "-----BEGIN OPENSSH PRIVATE KEY-----\n(leave blank to keep existing key)"
                            } else {
                                "-----BEGIN OPENSSH PRIVATE KEY-----"
                            },
                            oninput: move |evt| on_change.call(("credential_secret".to_string(), evt.value())),
                        }
                        div {
                            style: "margin-top: 6px; font-size: 11px; color: var(--cf-text-muted);",
                            "Paste an unencrypted SSH private key. Leave blank to keep the existing key."
                        }
                    }
                }
            }

            if credential_type == "pat" {
                div { style: "display: grid; gap: 10px;",
                    div { style: "display: grid; grid-template-columns: auto 1fr; gap: 10px; align-items: center;",
                        span { style: "font-size: 13px; color: var(--cf-text-secondary);", "Token Username (optional)" }
                        input {
                            class: "input focus-ring",
                            value: "{credential_username}",
                            placeholder: "oauth2",
                            oninput: move |evt| on_change.call(("credential_username".to_string(), evt.value())),
                        }
                    }
                    div { style: "display: grid; grid-template-columns: auto 1fr; gap: 10px; align-items: center;",
                        span { style: "font-size: 13px; color: var(--cf-text-secondary);", "Access Token" }
                        input {
                            class: "input focus-ring",
                            r#type: "password",
                            value: "{credential_secret}",
                            placeholder: if has_existing_secret { "•••••••• (leave blank to keep existing)" } else { "glpat-..." },
                            oninput: move |evt| on_change.call(("credential_secret".to_string(), evt.value())),
                        }
                    }
                }
            }

            if credential_type == "username_password" {
                div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 12px; align-items: end;",
                    label {
                        class: "field",
                        span { "Username" }
                        input {
                            class: "input focus-ring",
                            value: "{credential_username}",
                            oninput: move |evt| on_change.call(("credential_username".to_string(), evt.value())),
                        }
                    }
                    label {
                        class: "field",
                        span { "Password" }
                        input {
                            class: "input focus-ring",
                            r#type: "password",
                            value: "{credential_secret}",
                            oninput: move |evt| on_change.call(("credential_secret".to_string(), evt.value())),
                        }
                    }
                }
            }

            if credential_type == "none" {
                div {
                    style: "font-size: 12px; color: var(--cf-text-muted);",
                    "No auth — works for anonymous HTTPS clones and read-only public repos."
                }
            }

            
        }
    }
}

fn remove_flake_by_id(
    flakes: Signal<Vec<FlakeListItem>>,
    mut pending_remove: Signal<Option<FlakeListItem>>,
    flake_id: i32,
) {
    let target = flakes
        .read()
        .iter()
        .find(|flake| flake.id == flake_id)
        .cloned();
    if let Some(flake) = target {
        pending_remove.set(Some(flake));
    }
}

fn start_edit_flake(
    flakes: Signal<Vec<FlakeListItem>>,
    mut editing_flake: Signal<Option<EditFlakeDraft>>,
    mut edit_error: Signal<Option<String>>,
    flake_id: i32,
) {
    let target = flakes
        .read()
        .iter()
        .find(|flake| flake.id == flake_id)
        .cloned();
    if let Some(flake) = target {
        editing_flake.set(Some(EditFlakeDraft {
            id: flake.id,
            name: flake.name,
            repo_url: flake.repo_url,
            branch: flake.branch,
            environment: flake
                .environments
                .first()
                .cloned()
                .unwrap_or_else(|| "production".to_string()),
            description: String::new(),
            build_scope: flake.build_scope,
            credential_type: "none".to_string(),
            credential_username: String::new(),
            credential_secret: String::new(),
            credential_ssh_username: String::new(),
            has_existing_secret: false,
        }));
        edit_error.set(None);

        spawn(async move {
            match fetch_flake_credentials(flake_id).await {
                Ok(summary) => {
                    let current_value = editing_flake.read().clone();
                    if let Some(mut current) = current_value {
                        current.credential_type = normalize_credential_type(&summary.auth_type);
                        current.credential_username = summary.username.unwrap_or_default();
                        current.credential_ssh_username = summary.ssh_username.unwrap_or_default();
                        current.has_existing_secret = summary.has_secret;
                        editing_flake.set(Some(current));
                    }
                }
                Err(error) => {
                    edit_error.set(Some(format!("Failed to load credentials: {error}")));
                }
            }
        });
    }
}

async fn save_flake_credentials(
    flake_id: i32,
    draft: &impl FlakeCredentialDraft,
) -> Result<(), String> {
    if draft.credential_type() == "none" {
        if !draft.has_existing_secret() {
            return Ok(());
        }
        delete_flake_credentials(flake_id)
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    let secret = draft.credential_secret().trim().to_string();
    let request = CreateFlakeCredentialRequest {
        auth_type: draft.credential_type().to_string(),
        username: normalize_optional_value(draft.credential_username()),
        secret: if secret.is_empty() && draft.has_existing_secret() {
            None
        } else {
            normalize_optional_value(&secret)
        },
        ssh_username: normalize_optional_value(draft.credential_ssh_username()),
    };

    put_flake_credentials(flake_id, &request)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

trait FlakeCredentialDraft {
    fn credential_type(&self) -> &str;
    fn credential_username(&self) -> &str;
    fn credential_secret(&self) -> &str;
    fn credential_ssh_username(&self) -> &str;
    fn has_existing_secret(&self) -> bool;
}

impl FlakeCredentialDraft for NewFlakeDraft {
    fn credential_type(&self) -> &str {
        &self.credential_type
    }
    fn credential_username(&self) -> &str {
        &self.credential_username
    }
    fn credential_secret(&self) -> &str {
        &self.credential_secret
    }
    fn credential_ssh_username(&self) -> &str {
        &self.credential_ssh_username
    }
    fn has_existing_secret(&self) -> bool {
        false
    }
}

impl FlakeCredentialDraft for EditFlakeDraft {
    fn credential_type(&self) -> &str {
        &self.credential_type
    }
    fn credential_username(&self) -> &str {
        &self.credential_username
    }
    fn credential_secret(&self) -> &str {
        &self.credential_secret
    }
    fn credential_ssh_username(&self) -> &str {
        &self.credential_ssh_username
    }
    fn has_existing_secret(&self) -> bool {
        self.has_existing_secret
    }
}

fn normalize_optional_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_credential_type(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "none" => "none".to_string(),
        "ssh" | "ssh-key" | "ssh_key" => "ssh_key".to_string(),
        "pat" | "token" | "https_token" => "pat".to_string(),
        "username_password" => "username_password".to_string(),
        _ => "none".to_string(),
    }
}

fn refresh_flake_by_id(
    flake_id: i32,
    mut refreshing_flake: Signal<Option<i32>>,
    mut sync_note: Signal<Option<String>>,
) {
    use crate::api::client::refresh_flake;

    refreshing_flake.set(Some(flake_id));
    spawn(async move {
        match refresh_flake(flake_id).await {
            Ok(()) => {
                sync_note.set(Some("✅ Flake cache refreshed successfully".to_string()));
                refreshing_flake.set(None);
            }
            Err(e) => {
                sync_note.set(Some(format!("❌ Failed to refresh flake: {}", e)));
                refreshing_flake.set(None);
            }
        }
    });
}

fn validate_new_flake(draft: &NewFlakeDraft, existing: &[FlakeListItem]) -> Result<(), String> {
    let name = draft.name.trim();
    let repo_url = draft.repo_url.trim();
    let branch = draft.branch.trim();

    if name.is_empty() {
        return Err("Flake name is required.".to_string());
    }
    if repo_url.is_empty() {
        return Err("Repository URL is required.".to_string());
    }
    if !looks_like_repo_url(repo_url) {
        return Err("Repository URL must look like a git remote.".to_string());
    }
    if existing
        .iter()
        .any(|flake| flake.repo_url.eq_ignore_ascii_case(repo_url))
    {
        return Err("Repository URL already exists in the registry.".to_string());
    }
    if !branch.is_empty() && branch.contains(char::is_whitespace) {
        return Err("Branch must not contain whitespace.".to_string());
    }

    Ok(())
}

fn validate_flake_edit(draft: &EditFlakeDraft, existing: &[FlakeListItem]) -> Result<(), String> {
    let name = draft.name.trim();
    let repo_url = draft.repo_url.trim();
    let branch = draft.branch.trim();

    if name.is_empty() {
        return Err("Flake name is required.".to_string());
    }
    if repo_url.is_empty() {
        return Err("Repository URL is required.".to_string());
    }
    if !looks_like_repo_url(repo_url) {
        return Err("Repository URL must look like a git remote.".to_string());
    }
    if existing
        .iter()
        .any(|flake| flake.id != draft.id && flake.repo_url.eq_ignore_ascii_case(repo_url))
    {
        return Err("Repository URL already exists in the registry.".to_string());
    }
    if !branch.is_empty() && branch.contains(char::is_whitespace) {
        return Err("Branch must not contain whitespace.".to_string());
    }

    Ok(())
}

fn looks_like_repo_url(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("git@")
        || lower.starts_with("ssh://")
        || lower.starts_with("github:")
}

fn normalize_optional_branch(value: &str) -> Option<String> {
    let branch = value.trim();
    if branch.is_empty() {
        None
    } else {
        Some(branch.to_string())
    }
}

#[component]
fn EnvironmentFilterDropdown(
    environments: Vec<String>,
    selected: Signal<Vec<String>>,
    open_dropdown: Signal<Option<FilterDropdown>>,
) -> Element {
    let label = format_multi_label(&selected.read(), "All environments");
    let is_open = *open_dropdown.read() == Some(FilterDropdown::Environment);

    rsx! {
        div {
            class: "relative",
            button {
                class: "w-full flex items-center justify-between rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                onclick: move |_| {
                    if is_open {
                        open_dropdown.set(None);
                    } else {
                        open_dropdown.set(Some(FilterDropdown::Environment));
                    }
                },
                span { "{label}" }
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M19 9l-7 7-7-7" }
                }
            }

            if is_open {
                div {
                    class: "absolute left-0 right-0 mt-1 rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} shadow-xl z-[3000]",
                    button {
                        class: "w-full text-left px-3 py-2 text-sm hover:bg-gray-700",
                        onclick: move |_| {
                            selected.set(Vec::new());
                            open_dropdown.set(None);
                        },
                        "All environments"
                    }
                    for env in environments {
                        {
                            let is_selected = selected.read().contains(&env);
                            let env_clone = env.clone();
                            rsx! {
                                button {
                                    key: "{env_clone}",
                                    class: "w-full flex items-center gap-2 px-3 py-2 text-sm hover:bg-gray-700",
                                    onclick: move |_| {
                                        let mut next = selected.read().clone();
                                        if next.contains(&env_clone) {
                                            next.retain(|value| value != &env_clone);
                                        } else {
                                            next.push(env_clone.clone());
                                        }
                                        selected.set(next);
                                    },
                                    div {
                                        class: "w-4 h-4 rounded border flex items-center justify-center",
                                        class: if is_selected { "bg-blue-500 border-blue-500" } else { "border-gray-500" },
                                        if is_selected {
                                            svg {
                                                class: "w-3 h-3 text-white",
                                                fill: "none",
                                                stroke: "currentColor",
                                                view_box: "0 0 24 24",
                                                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "3", d: "M5 13l4 4L19 7" }
                                            }
                                        }
                                    }
                                    span { "{env_clone}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CommitFilterDropdown(
    selected: Signal<Vec<CommitFilter>>,
    open_dropdown: Signal<Option<FilterDropdown>>,
) -> Element {
    let label = format_commit_label(&selected.read());
    let options = vec![CommitFilter::HasCommit, CommitFilter::NoCommit];
    let is_open = *open_dropdown.read() == Some(FilterDropdown::Commit);

    rsx! {
        div {
            class: "relative",
            button {
                class: "w-full flex items-center justify-between rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                onclick: move |_| {
                    if is_open {
                        open_dropdown.set(None);
                    } else {
                        open_dropdown.set(Some(FilterDropdown::Commit));
                    }
                },
                span { "{label}" }
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M19 9l-7 7-7-7" }
                }
            }
            if is_open {
                div {
                    class: "absolute left-0 right-0 mt-1 rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} shadow-xl z-[3000]",
                    button {
                        class: "w-full text-left px-3 py-2 text-sm hover:bg-gray-700",
                        onclick: move |_| {
                            selected.set(Vec::new());
                            open_dropdown.set(None);
                        },
                        "All commit states"
                    }
                    for status in options {
                        {
                            let is_selected = selected.read().contains(&status);
                            let label = status.label();
                            rsx! {
                                button {
                                    key: "{label}",
                                    class: "w-full flex items-center gap-2 px-3 py-2 text-sm hover:bg-gray-700",
                                    onclick: move |_| {
                                        let mut next = selected.read().clone();
                                        if next.contains(&status) {
                                            next.retain(|value| value != &status);
                                        } else {
                                            next.push(status);
                                        }
                                        selected.set(next);
                                    },
                                    div {
                                        class: "w-4 h-4 rounded border flex items-center justify-center",
                                        class: if is_selected { "bg-blue-500 border-blue-500" } else { "border-gray-500" },
                                        if is_selected {
                                            svg {
                                                class: "w-3 h-3 text-white",
                                                fill: "none",
                                                stroke: "currentColor",
                                                view_box: "0 0 24 24",
                                                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "3", d: "M5 13l4 4L19 7" }
                                            }
                                        }
                                    }
                                    span { "{label}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SizeFilterDropdown(
    selected: Signal<Vec<SizeBucket>>,
    open_dropdown: Signal<Option<FilterDropdown>>,
) -> Element {
    let label = format_size_label(&selected.read());
    let options = vec![SizeBucket::Small, SizeBucket::Medium, SizeBucket::Large];
    let is_open = *open_dropdown.read() == Some(FilterDropdown::Size);

    rsx! {
        div {
            class: "relative",
            button {
                class: "w-full flex items-center justify-between rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                onclick: move |_| {
                    if is_open {
                        open_dropdown.set(None);
                    } else {
                        open_dropdown.set(Some(FilterDropdown::Size));
                    }
                },
                span { "{label}" }
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M19 9l-7 7-7-7" }
                }
            }
            if is_open {
                div {
                    class: "absolute left-0 right-0 mt-1 rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} shadow-xl z-[3000]",
                    button {
                        class: "w-full text-left px-3 py-2 text-sm hover:bg-gray-700",
                        onclick: move |_| {
                            selected.set(Vec::new());
                            open_dropdown.set(None);
                        },
                        "All sizes"
                    }
                    for bucket in options {
                        {
                            let is_selected = selected.read().contains(&bucket);
                            let label = bucket.label();
                            rsx! {
                                button {
                                    key: "{label}",
                                    class: "w-full flex items-center gap-2 px-3 py-2 text-sm hover:bg-gray-700",
                                    onclick: move |_| {
                                        let mut next = selected.read().clone();
                                        if next.contains(&bucket) {
                                            next.retain(|value| value != &bucket);
                                        } else {
                                            next.push(bucket);
                                        }
                                        selected.set(next);
                                    },
                                    div {
                                        class: "w-4 h-4 rounded border flex items-center justify-center",
                                        class: if is_selected { "bg-blue-500 border-blue-500" } else { "border-gray-500" },
                                        if is_selected {
                                            svg {
                                                class: "w-3 h-3 text-white",
                                                fill: "none",
                                                stroke: "currentColor",
                                                view_box: "0 0 24 24",
                                                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "3", d: "M5 13l4 4L19 7" }
                                            }
                                        }
                                    }
                                    span { "{label}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SortableHeader(
    label: &'static str,
    column: SortColumn,
    current_col: Option<SortColumn>,
    current_dir: SortDirection,
    sort_column: Signal<Option<SortColumn>>,
    sort_direction: Signal<SortDirection>,
) -> Element {
    let is_active = current_col == Some(column);
    let icon = if is_active {
        match current_dir {
            SortDirection::Asc => "↑",
            SortDirection::Desc => "↓",
        }
    } else {
        "↕"
    };

    rsx! {
        th {
            class: "px-4 py-3 text-left text-xs font-medium text-gray-400 uppercase tracking-wider cursor-pointer hover:text-white transition select-none",
            onclick: move |_| {
                if current_col == Some(column) {
                    let new_dir = current_dir.toggle();
                    sort_direction.set(new_dir);
                } else {
                    sort_column.set(Some(column));
                    sort_direction.set(SortDirection::Asc);
                }
            },
            span { class: "inline-flex items-center gap-1",
                "{label}"
                span { class: if is_active { "text-blue-400" } else { "text-gray-600" }, "{icon}" }
            }
        }
    }
}

fn matches_environment(flake: &FlakeListItem, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }

    filters.iter().any(|filter| {
        flake
            .environments
            .iter()
            .any(|env| env.eq_ignore_ascii_case(filter))
    })
}

fn matches_commit_state(flake: &FlakeListItem, filters: &[CommitFilter]) -> bool {
    if filters.is_empty() {
        return true;
    }

    let has_commit = flake.latest_commit.is_some();
    filters.iter().any(|filter| match filter {
        CommitFilter::HasCommit => has_commit,
        CommitFilter::NoCommit => !has_commit,
    })
}

fn matches_size(flake: &FlakeListItem, filters: &[SizeBucket]) -> bool {
    if filters.is_empty() {
        return true;
    }

    filters
        .iter()
        .any(|bucket| bucket.matches(flake.system_count))
}

fn matches_search(flake: &FlakeListItem, query: &str) -> bool {
    if query.trim().is_empty() {
        return true;
    }

    let query = query.to_lowercase();
    flake.name.to_lowercase().contains(&query) || flake.repo_url.to_lowercase().contains(&query)
}

fn environments_label(flake: &FlakeListItem) -> String {
    if flake.environments.is_empty() {
        "-".to_string()
    } else {
        flake.environments.join(", ")
    }
}

fn latest_commit_label(flake: &FlakeListItem) -> String {
    flake
        .latest_commit
        .clone()
        .unwrap_or_else(|| "-".to_string())
}

fn format_multi_label(values: &[String], placeholder: &str) -> String {
    if values.is_empty() {
        placeholder.to_string()
    } else if values.len() == 1 {
        values[0].clone()
    } else {
        format!("{} selected", values.len())
    }
}

fn format_commit_label(values: &[CommitFilter]) -> String {
    if values.is_empty() {
        "All commit states".to_string()
    } else if values.len() == 1 {
        values[0].label().to_string()
    } else {
        format!("{} selected", values.len())
    }
}

fn format_size_label(values: &[SizeBucket]) -> String {
    if values.is_empty() {
        "All sizes".to_string()
    } else if values.len() == 1 {
        values[0].label().to_string()
    } else {
        format!("{} selected", values.len())
    }
}

fn unique_environments(flakes: &[FlakeListItem]) -> Vec<String> {
    let mut values: Vec<String> = flakes
        .iter()
        .flat_map(|flake| flake.environments.clone())
        .collect();

    values.sort();
    values.dedup();
    values
}

fn mock_flakes() -> Vec<FlakeListItem> {
    let mut flakes: HashMap<i32, FlakeListItem> = HashMap::new();

    for system in mock_system_details() {
        let Some(flake) = system.flake else {
            continue;
        };

        let entry = flakes.entry(flake.id).or_insert_with(|| FlakeListItem {
            id: flake.id,
            name: flake.name.clone(),
            repo_url: flake.repo_url.clone(),
            branch: "main".to_string(),
            build_scope: "cf_systems_only".to_string(),
            latest_commit: flake.latest_commit.clone(),
            system_count: 0,
            environments: Vec::new(),
            last_synced_at: Utc::now() - Duration::minutes(37),
        });

        entry.system_count += 1;
        if let Some(environment) = system.environment {
            if !entry
                .environments
                .iter()
                .any(|env| env.eq_ignore_ascii_case(&environment))
            {
                entry.environments.push(environment);
            }
        }
    }

    let mut values: Vec<FlakeListItem> = flakes.into_values().collect();
    for flake in &mut values {
        flake.environments.sort();
        flake.environments.dedup();
    }

    // Keep one flake intentionally stale so manual sync shows visible updates in mock mode.
    if let Some(stale) = values
        .iter_mut()
        .find(|flake| flake.name == "infrastructure")
    {
        stale.latest_commit = Some("c3d4e5f".to_string());
    }

    values.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    values
}

fn prefers_view_from_query() -> Option<FlakesViewMode> {
    let location = window()?.location();
    let search = location.search().ok().unwrap_or_default();
    let hash = location.hash().ok().unwrap_or_default();
    let combined = format!("{search}{hash}");

    if combined.contains("view=cards") {
        return Some(FlakesViewMode::Cards);
    }
    if combined.contains("view=table") {
        return Some(FlakesViewMode::Table);
    }
    None
}

fn table_class(active: bool) -> &'static str {
    if active {
        "bg-blue-600/30 text-white"
    } else {
        "text-gray-400 hover:text-white"
    }
}

fn merge_flake_timeline_batches(
    current: Vec<FlakeTimeline>,
    incoming: Vec<FlakeTimeline>,
    ordered_ids: &[i32],
) -> Vec<FlakeTimeline> {
    let mut by_flake: HashMap<i32, FlakeTimeline> = current
        .into_iter()
        .map(|timeline| (timeline.flake_id, timeline))
        .collect();

    for timeline in incoming {
        by_flake.insert(timeline.flake_id, timeline);
    }

    ordered_ids
        .iter()
        .filter_map(|flake_id| by_flake.remove(flake_id))
        .collect()
}

fn sync_flake_registry(flakes: &mut [FlakeListItem], timelines: &[FlakeTimeline]) -> usize {
    let now = Utc::now();
    let latest_by_id: HashMap<i32, String> = timelines
        .iter()
        .filter_map(|timeline| {
            timeline
                .commits
                .first()
                .map(|commit| (timeline.flake_id, commit.hash.chars().take(7).collect()))
        })
        .collect();

    let mut changed = 0;
    for flake in flakes.iter_mut() {
        if let Some(latest) = latest_by_id.get(&flake.id) {
            if flake.latest_commit.as_deref() != Some(latest.as_str()) {
                flake.latest_commit = Some(latest.clone());
                changed += 1;
            }
        }
        flake.last_synced_at = now;
    }

    changed
}

fn sync_single_flake_registry(
    flakes: &mut [FlakeListItem],
    timelines: &[FlakeTimeline],
    flake_id: i32,
) -> usize {
    let now = Utc::now();
    let latest = timelines
        .iter()
        .find(|timeline| timeline.flake_id == flake_id)
        .and_then(|timeline| {
            timeline
                .commits
                .first()
                .map(|commit| commit.hash.chars().take(7).collect::<String>())
        });

    let Some(flake) = flakes.iter_mut().find(|flake| flake.id == flake_id) else {
        return 0;
    };

    let mut changed = 0;
    if let Some(latest_commit) = latest {
        if flake.latest_commit.as_deref() != Some(latest_commit.as_str()) {
            flake.latest_commit = Some(latest_commit);
            changed = 1;
        }
    }
    flake.last_synced_at = now;
    changed
}
fn build_flake_commits(timelines: &[FlakeTimeline], flake_id: i32) -> Vec<FlakeHistoryCommit> {
    let Some(timeline) = timelines
        .iter()
        .find(|timeline| timeline.flake_id == flake_id)
    else {
        return Vec::new();
    };

    timeline
        .commits
        .iter()
        .map(|commit| {
            let short_hash = commit.hash.chars().take(7).collect::<String>();
            let total_system_count = usize::try_from(commit.system_count)
                .ok()
                .unwrap_or(0)
                .max(commit.systems.len())
                .max(commit.system_paths.len());
            let systems = commit
                .systems
                .iter()
                .take(MAX_SYSTEMS_STORED_PER_COMMIT)
                .cloned()
                .collect();
            let system_paths = commit
                .system_paths
                .iter()
                .take(MAX_SYSTEMS_STORED_PER_COMMIT)
                .cloned()
                .collect();
            FlakeHistoryCommit {
                id: commit.id,
                hash: commit.hash.clone(),
                message: normalize_commit_message(&commit.message, &short_hash),
                author: normalize_commit_author(&commit.author),
                committed_at: commit.committed_at,
                files_changed: 0,
                insertions: 0,
                deletions: 0,
                diff: String::new(),
                systems,
                system_paths,
                total_system_count,
                build_status: commit.build_status.clone(),
                evaluation_status: commit.evaluation_status.clone(),
                evaluation_error_message: commit.evaluation_error_message.clone(),
            }
        })
        .collect()
}

fn eval_badge_label(status: Option<&str>) -> &'static str {
    match status {
        Some("in_progress") => "running",
        Some("pending") => "queued",
        Some("failed") => "failed",
        Some("complete") => "complete",
        _ => "idle",
    }
}

fn eval_badge_style(status: Option<&str>) -> &'static str {
    match status {
        Some("in_progress") => "background-color: #1f3d52; border-color: #3b82f6; color: #dbeafe;",
        Some("pending") => "background-color: #3a3120; border-color: #d97706; color: #fef3c7;",
        Some("failed") => "background-color: #472726; border-color: #ef4444; color: #fee2e2;",
        Some("complete") => "background-color: #1f3a2f; border-color: #22c55e; color: #dcfce7;",
        _ => "background-color: #2b303b; border-color: #495264; color: #cbd5e1;",
    }
}

fn build_badge_label(status: &ApiBuildStatus) -> &'static str {
    match status {
        ApiBuildStatus::Queued => "queued",
        ApiBuildStatus::Building => "running",
        ApiBuildStatus::Cancelling => "stopping",
        ApiBuildStatus::Cancelled => "cancelled",
        ApiBuildStatus::Failed => "failed",
        ApiBuildStatus::Complete => "complete",
        ApiBuildStatus::Idle => "idle",
        ApiBuildStatus::Cancelling => "stopping",
        ApiBuildStatus::Cancelled => "cancelled",
    }
}

fn build_badge_style(status: &ApiBuildStatus) -> &'static str {
    match status {
        ApiBuildStatus::Queued => {
            "background-color: #3a3120; border-color: #d97706; color: #fef3c7;"
        }
        ApiBuildStatus::Building => {
            "background-color: #1f3d52; border-color: #3b82f6; color: #dbeafe;"
        }
        ApiBuildStatus::Cancelling => {
            "background-color: #3d2f1f; border-color: #fb923c; color: #fed7aa;"
        }
        ApiBuildStatus::Cancelled => {
            "background-color: #2b303b; border-color: #6b7280; color: #9ca3af;"
        }
        ApiBuildStatus::Failed => {
            "background-color: #472726; border-color: #ef4444; color: #fee2e2;"
        }
        ApiBuildStatus::Complete => {
            "background-color: #1f3a2f; border-color: #22c55e; color: #dcfce7;"
        }
        ApiBuildStatus::Idle => "background-color: #2b303b; border-color: #495264; color: #cbd5e1;",
        ApiBuildStatus::Cancelling => {
            "background-color: #3d2f1f; border-color: #fb923c; color: #fed7aa;"
        }
        ApiBuildStatus::Cancelled => {
            "background-color: #2b303b; border-color: #6b7280; color: #9ca3af;"
        }
    }
}

fn system_chip_style(status: Option<&crate::hooks::websocket::SystemEvalStatus>) -> &'static str {
    match status {
        Some(crate::hooks::websocket::SystemEvalStatus::Success) => {
            "background-color: #163b2b; border-color: #22c55e; color: #dcfce7;"
        }
        Some(crate::hooks::websocket::SystemEvalStatus::Failed) => {
            "background-color: #4a2324; border-color: #ef4444; color: #fee2e2;"
        }
        Some(crate::hooks::websocket::SystemEvalStatus::PolicyFailed) => {
            "background-color: #4a2f18; border-color: #f59e0b; color: #ffedd5;"
        }
        Some(crate::hooks::websocket::SystemEvalStatus::QueuedForBuild) => {
            "background-color: #1a3d3b; border-color: #10b981; color: #d1fae5;"
        }
        Some(crate::hooks::websocket::SystemEvalStatus::Evaluating) => {
            "background-color: #4a3a16; border-color: #facc15; color: #fef9c3;"
        }
        Some(crate::hooks::websocket::SystemEvalStatus::Pending) | None => {
            "background-color: #2b303b; border-color: #64748b; color: #cbd5e1;"
        }
    }
}

fn normalize_commit_message(message: &str, short_hash: &str) -> String {
    let cleaned = message.trim();
    if cleaned.is_empty() {
        return format!("Commit {short_hash}");
    }

    cleaned
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_commit_author(author: &str) -> String {
    let cleaned = author.trim();
    if cleaned.is_empty() {
        "Unknown author".to_string()
    } else {
        cleaned.to_string()
    }
}

fn commit_message_lines(message: &str, headline_limit: usize) -> (String, Option<String>) {
    let mut lines: Vec<String> = message
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect();

    if lines.is_empty() {
        return ("Commit".to_string(), None);
    }

    let first = lines.remove(0);
    if !lines.is_empty() {
        let secondary = lines.join(" ");
        return (
            truncate_with_ellipsis(&first, headline_limit),
            Some(truncate_with_ellipsis(&secondary, 220)),
        );
    }

    if first.chars().count() > headline_limit {
        let headline = truncate_with_ellipsis(&first, headline_limit);
        let remainder = first
            .chars()
            .skip(headline_limit)
            .collect::<String>()
            .trim()
            .to_string();
        if remainder.is_empty() {
            (headline, None)
        } else {
            (headline, Some(truncate_with_ellipsis(&remainder, 220)))
        }
    } else {
        (first, None)
    }
}

fn truncate_with_ellipsis(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let chars: Vec<char> = input.chars().collect();
    if chars.len() <= max_chars {
        return input.to_string();
    }

    let cutoff = max_chars.saturating_sub(1);
    let mut truncated = chars[..cutoff].iter().collect::<String>();
    truncated = truncated.trim_end().to_string();
    format!("{truncated}…")
}

fn full_diff_for_commit(flake_name: &str, commit: &crate::api::models::FlakeCommit) -> String {
    let selector = commit.hash.bytes().last().unwrap_or(b'0') % 4;
    match selector {
        0 => format!(
            "diff --git a/hosts/{flake_name}/default.nix b/hosts/{flake_name}/default.nix\n\
index 2b0fa11..71c3d97 100644\n\
--- a/hosts/{flake_name}/default.nix\n\
+++ b/hosts/{flake_name}/default.nix\n\
@@ -18,8 +18,12 @@ in {{\n\
   services.openssh.enable = true;\n\
-  services.openssh.settings.PasswordAuthentication = true;\n\
+  services.openssh.settings.PasswordAuthentication = false;\n\
+  services.openssh.settings.KbdInteractiveAuthentication = false;\n\
+  services.openssh.ports = [ 22 2222 ];\n\
\n\
   environment.systemPackages = with pkgs; [\n\
     git\n\
+    htop\n\
   ];\n\
@@ -42,6 +46,10 @@ in {{\n\
   systemd.services.crystal-forge-agent = {{\n\
     wantedBy = [ \"multi-user.target\" ];\n\
+    serviceConfig = {{\n\
+      Restart = \"always\";\n\
+      RestartSec = \"8s\";\n\
+    }};\n\
   }};\n\
 }}\n\
\n\
// {message}\n",
            flake_name = flake_name,
            message = commit.message
        ),
        1 => format!(
            "diff --git a/modules/networking.nix b/modules/networking.nix\n\
index 618a22d..7d8a1a4 100644\n\
--- a/modules/networking.nix\n\
+++ b/modules/networking.nix\n\
@@ -10,9 +10,11 @@\n\
 {{ config, lib, ... }}: {{\n\
   networking.firewall = {{\n\
-    enable = false;\n\
+    enable = true;\n\
     allowedTCPPorts = [ 22 443 9100 ];\n\
+    allowedUDPPorts = [ 51820 ];\n\
   }};\n\
\n\
-  networking.useNetworkd = false;\n\
+  networking.useNetworkd = true;\n\
 }}\n\
\n\
diff --git a/overlays/default.nix b/overlays/default.nix\n\
index 77ea010..7ccca12 100644\n\
--- a/overlays/default.nix\n\
+++ b/overlays/default.nix\n\
@@ -1,5 +1,9 @@\n\
 final: prev: {{\n\
+  crystal-forge-agent = prev.crystal-forge-agent.overrideAttrs (_: {{\n\
+    RUST_LOG = \"info\";\n\
+  }});\n\
+\n\
   jq = prev.jq;\n\
 }}\n\
\n\
// {message}\n",
            message = commit.message
        ),
        2 => format!(
            "diff --git a/flake.nix b/flake.nix\n\
index 1aaa010..3bbb120 100644\n\
--- a/flake.nix\n\
+++ b/flake.nix\n\
@@ -8,10 +8,12 @@\n\
   inputs = {{\n\
-    nixpkgs.url = \"github:NixOS/nixpkgs/nixos-25.05\";\n\
+    nixpkgs.url = \"github:NixOS/nixpkgs/nixos-25.11\";\n\
     snowfall-lib.url = \"github:snowfallorg/lib\";\n\
   }};\n\
\n\
   outputs = inputs @ {{ self, nixpkgs, ... }}: let\n\
+    systems = [ \"x86_64-linux\" \"aarch64-linux\" ];\n\
   in {{\n\
-    packages.x86_64-linux.default = ...;\n\
+    packages = builtins.listToAttrs (map (system: {{\n\
+      name = system;\n\
+      value.default = ...;\n\
+    }}) systems);\n\
   }};\n\
\n\
// {message}\n",
            message = commit.message
        ),
        _ => format!(
            "diff --git a/services/web.nix b/services/web.nix\n\
index 04fed11..6acdd22 100644\n\
--- a/services/web.nix\n\
+++ b/services/web.nix\n\
@@ -4,12 +4,15 @@\n\
 {{ config, pkgs, ... }}: {{\n\
   services.nginx = {{\n\
     enable = true;\n\
-    recommendedTlsSettings = false;\n\
+    recommendedTlsSettings = true;\n\
     recommendedOptimisation = true;\n\
+    clientMaxBodySize = \"64m\";\n\
   }};\n\
\n\
   systemd.services.nginx-reload = {{\n\
     serviceConfig.Type = \"oneshot\";\n\
     script = ''\n\
       set -euo pipefail\n\
+      ${{pkgs.nginx}}/bin/nginx -t\n\
       systemctl reload nginx\n\
     '';\n\
   }};\n\
 }}\n\
\n\
// {message}\n",
            message = commit.message
        ),
    }
}

fn diff_stats(diff: &str) -> (usize, usize, usize) {
    let mut files_changed = 0;
    let mut insertions = 0;
    let mut deletions = 0;

    for line in diff.lines() {
        if line.starts_with("diff --git") {
            files_changed += 1;
        } else if line.starts_with('+') && !line.starts_with("+++") {
            insertions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
        }
    }

    (files_changed, insertions, deletions)
}

fn diff_file_stats(file: &ParsedDiffFile) -> (usize, usize) {
    let mut insertions = 0;
    let mut deletions = 0;

    for line in &file.lines {
        if line.prefix == '+' {
            insertions += 1;
        } else if line.prefix == '-' {
            deletions += 1;
        }
    }

    (insertions, deletions)
}

fn diff_file_label(file: &ParsedDiffFile) -> String {
    if file.new_path == "/dev/null" {
        format!("{} (deleted)", file.old_path)
    } else if file.old_path == "/dev/null" {
        format!("{} (new)", file.new_path)
    } else if file.new_path == file.old_path {
        file.new_path.clone()
    } else {
        format!("{} -> {}", file.old_path, file.new_path)
    }
}

fn parse_unified_diff(diff: &str) -> Vec<ParsedDiffFile> {
    let mut files = Vec::new();
    let mut current_block: Vec<String> = Vec::new();

    for line in diff.lines() {
        if line.starts_with("diff --git ") && !current_block.is_empty() {
            files.push(parse_diff_file_block(&current_block));
            current_block.clear();
        }
        current_block.push(line.to_string());
    }

    if !current_block.is_empty() {
        files.push(parse_diff_file_block(&current_block));
    }

    files
}

fn parse_diff_file_block(lines: &[String]) -> ParsedDiffFile {
    let mut old_path = String::new();
    let mut new_path = String::new();
    let mut payload = Vec::new();

    for line in lines {
        if let Some(path) = line.strip_prefix("--- ") {
            old_path = path.trim().trim_start_matches("a/").to_string();
        } else if let Some(path) = line.strip_prefix("+++ ") {
            new_path = path.trim().trim_start_matches("b/").to_string();
        } else {
            payload.push(line.clone());
        }
    }

    // If --- and +++ were not found, try to parse from "diff --git a/path b/path" header
    if old_path.is_empty() && new_path.is_empty() {
        if let Some(first_line) = lines.first() {
            if let Some(rest) = first_line.strip_prefix("diff --git ") {
                // Format: "a/path b/path" or "a/path b/other_path"
                let parts: Vec<&str> = rest.splitn(2, " b/").collect();
                if parts.len() == 2 {
                    old_path = parts[0].trim_start_matches("a/").to_string();
                    new_path = parts[1].to_string();
                }
            }
        }
    }

    if old_path.is_empty() && new_path.is_empty() {
        old_path = "(unknown)".to_string();
        new_path = "(unknown)".to_string();
    }

    let language = if new_path != "(unknown)" {
        detect_language(&new_path)
    } else {
        detect_language(&old_path)
    };

    ParsedDiffFile {
        old_path,
        new_path,
        language,
        lines: render_diff_lines(&payload),
    }
}

fn render_diff_lines(lines: &[String]) -> Vec<RenderedDiffLine> {
    let mut rendered = Vec::new();
    let mut old_line = None::<usize>;
    let mut new_line = None::<usize>;

    for line in lines {
        if let Some((old_start, new_start)) = parse_hunk_header(line) {
            old_line = Some(old_start);
            new_line = Some(new_start);
            rendered.push(RenderedDiffLine {
                old_number: None,
                new_number: None,
                prefix: '@',
                content: line.clone(),
                class_name: "bg-sky-500/10 border-y border-sky-500/20",
                is_hunk_header: true,
            });
            continue;
        }

        if line.starts_with('+') && !line.starts_with("+++") {
            let content = line.trim_start_matches('+').to_string();
            let next_new = new_line;
            if let Some(value) = new_line.as_mut() {
                *value += 1;
            }
            rendered.push(RenderedDiffLine {
                old_number: None,
                new_number: next_new,
                prefix: '+',
                content,
                class_name: "bg-emerald-500/10",
                is_hunk_header: false,
            });
            continue;
        }

        if line.starts_with('-') && !line.starts_with("---") {
            let content = line.trim_start_matches('-').to_string();
            let next_old = old_line;
            if let Some(value) = old_line.as_mut() {
                *value += 1;
            }
            rendered.push(RenderedDiffLine {
                old_number: next_old,
                new_number: None,
                prefix: '-',
                content,
                class_name: "bg-red-500/10",
                is_hunk_header: false,
            });
            continue;
        }

        if let Some(content) = line.strip_prefix(' ') {
            let next_old = old_line;
            let next_new = new_line;
            if let Some(value) = old_line.as_mut() {
                *value += 1;
            }
            if let Some(value) = new_line.as_mut() {
                *value += 1;
            }
            rendered.push(RenderedDiffLine {
                old_number: next_old,
                new_number: next_new,
                prefix: ' ',
                content: content.to_string(),
                class_name: "bg-transparent",
                is_hunk_header: false,
            });
            continue;
        }

        rendered.push(RenderedDiffLine {
            old_number: None,
            new_number: None,
            prefix: ' ',
            content: line.clone(),
            class_name: "bg-gray-900/50",
            is_hunk_header: false,
        });
    }

    rendered
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
    if !line.starts_with("@@") {
        return None;
    }

    let mut parts = line.split_whitespace();
    let _ = parts.next();
    let old_part = parts.next()?;
    let new_part = parts.next()?;

    let old_start = old_part
        .trim_start_matches('-')
        .split(',')
        .next()?
        .parse::<usize>()
        .ok()?;
    let new_start = new_part
        .trim_start_matches('+')
        .split(',')
        .next()?
        .parse::<usize>()
        .ok()?;

    Some((old_start, new_start))
}

fn detect_language(path: &str) -> &'static str {
    if path.ends_with(".nix") {
        "nix"
    } else if path.ends_with(".rs") {
        "rust"
    } else if path.ends_with(".toml") {
        "toml"
    } else if path.ends_with(".json") {
        "json"
    } else if path.ends_with(".yaml") || path.ends_with(".yml") {
        "yaml"
    } else if path.ends_with(".sh") {
        "bash"
    } else {
        "plaintext"
    }
}

fn extract_history_rewrite_conflict(
    error: &ApiClientError,
    selected_flake_id: Option<i32>,
) -> Option<(i32, String)> {
    let flake_id = selected_flake_id?;
    match error {
        ApiClientError::Status { code, body } =>
        {
            let body_lower = body.to_ascii_lowercase();
            let rewrite_marker = body_lower.contains("history rewrite")
                || body_lower.contains("history_rewrite_detected");
            let sync_failure_marker = body_lower.contains("failed to sync")
                || body_lower.contains("force push")
                || body_lower.contains("non-fast-forward");

            let looks_like_rewrite_conflict = (*code == 409 && rewrite_marker)
                || (*code == 500 && (rewrite_marker || sync_failure_marker));

            if !looks_like_rewrite_conflict {
                return None;
            }

            #[cfg(target_arch = "wasm32")]
            console::warn_1(
                &format!(
                    "[CF] history rewrite conflict detected for flake_id={}: {}",
                    flake_id, body
                )
                .into(),
            );
            Some((flake_id, body.clone()))
        }
        _ => None,
    }
}

fn is_commit_not_found_diff_error(error: &ApiClientError) -> bool {
    match error {
        ApiClientError::Status { body, .. } => {
            let lower = body.to_ascii_lowercase();
            lower.contains("failed to fetch diff for commit")
                || lower.contains("could not find commit")
                || lower.contains("commit_diff_unavailable")
                || lower.contains("could not be resolved in the current repository history")
        }
        _ => false,
    }
}

#[cfg(target_arch = "wasm32")]
fn highlight_diff_fragment(language: &str, text: &str) -> String {
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
fn highlight_diff_fragment(_language: &str, text: &str) -> String {
    escape_html(text)
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_commit_message_uses_hash_placeholder_when_empty() {
        assert_eq!(normalize_commit_message("   ", "abc1234"), "Commit abc1234");
    }

    #[test]
    fn commit_message_lines_splits_multiline_messages() {
        let (headline, secondary) =
            commit_message_lines("Improve sync error handling\n\nAlso tighten validation", 80);
        assert_eq!(headline, "Improve sync error handling");
        assert_eq!(secondary, Some("Also tighten validation".to_string()));
    }

    #[test]
    fn commit_message_lines_truncates_long_single_line() {
        let message = "this is a very long commit subject that should be truncated for compact cards and still retain a readable continuation";
        let (headline, secondary) = commit_message_lines(message, 40);
        assert!(headline.ends_with('…'));
        assert!(secondary.is_some());
    }

    #[test]
    fn normalize_commit_author_falls_back_for_empty_value() {
        assert_eq!(normalize_commit_author("  \n"), "Unknown author");
    }

    #[test]
    fn build_flake_commits_only_maps_requested_flake() {
        use crate::api::models::{BuildStatus, FlakeCommit, FlakeTimeline};

        let timelines = vec![
            FlakeTimeline {
                flake_id: 10,
                flake_name: "alpha".to_string(),
                repo_url: "https://example.com/alpha.git".to_string(),
                commits: vec![FlakeCommit {
                    id: 1,
                    hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                    message: "alpha commit".to_string(),
                    author: "Alice".to_string(),
                    committed_at: Utc::now(),
                    system_count: 1,
                    commits_behind: 0,
                    systems: vec!["alpha-host".to_string()],
                    system_paths: vec![],
                    build_status: Some(BuildStatus::Queued),
                    evaluation_status: Some("pending".to_string()),
                    evaluation_error_message: None,
                }],
            },
            FlakeTimeline {
                flake_id: 20,
                flake_name: "beta".to_string(),
                repo_url: "https://example.com/beta.git".to_string(),
                commits: vec![FlakeCommit {
                    id: 2,
                    hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                    message: "beta commit".to_string(),
                    author: "Bob".to_string(),
                    committed_at: Utc::now(),
                    system_count: 1,
                    commits_behind: 0,
                    systems: vec!["beta-host".to_string()],
                    system_paths: vec![],
                    build_status: Some(BuildStatus::Complete),
                    evaluation_status: Some("complete".to_string()),
                    evaluation_error_message: None,
                }],
            },
        ];

        let commits = build_flake_commits(&timelines, 20);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].id, 2);
        assert_eq!(commits[0].author, "Bob");
    }

    #[test]
    fn build_flake_commits_preserves_system_path_details() {
        use crate::api::models::{FlakeCommit, FlakeCommitSystemPath, FlakeTimeline};

        let timelines = vec![FlakeTimeline {
            flake_id: 42,
            flake_name: "gamma".to_string(),
            repo_url: "https://example.com/gamma.git".to_string(),
            commits: vec![FlakeCommit {
                id: 9,
                hash: "9999999999999999999999999999999999999999".to_string(),
                message: "gamma commit".to_string(),
                author: "Gina".to_string(),
                committed_at: Utc::now(),
                system_count: 1,
                commits_behind: 0,
                systems: vec!["gamma-host [CF system]".to_string()],
                system_paths: vec![FlakeCommitSystemPath {
                    config_name: "gamma-host".to_string(),
                    is_cf_system: true,
                    cf_hostname: Some("gamma-host".to_string()),
                    mapped_host_count: 1,
                    expected_store_path: Some("/nix/store/expected-gamma".to_string()),
                    current_store_path: Some("/nix/store/current-gamma".to_string()),
                    cve_scan_eligible: true,
                    cve_scan_blocked_reason: None,
                }],
                build_status: None,
                evaluation_status: None,
                evaluation_error_message: None,
            }],
        }];

        let commits = build_flake_commits(&timelines, 42);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].system_paths.len(), 1);
        assert_eq!(
            commits[0].system_paths[0].expected_store_path.as_deref(),
            Some("/nix/store/expected-gamma")
        );
        assert_eq!(
            commits[0].system_paths[0].current_store_path.as_deref(),
            Some("/nix/store/current-gamma")
        );
    }
}

// ============================================================================
// NEW IMPLEMENTATION - Phase 1: PageHeader + FilterBar
// Matching FlakesView.jsx lines 24-52 EXACTLY
// ============================================================================

// ============================================================================
// FlakesListViewNew - Complete UI implementation matching JSX design
// ============================================================================
//
// INTEGRATION STATUS: UI Complete, API Ready
//
// This component implements the complete FlakesView JSX design with all visual
// components functional. To wire to real API data, follow this integration guide:
//
// PHASE 7-8 INTEGRATION CHECKLIST:
//
// 1. Replace MockFlakeItem with FlakeRegistryItem from api::models
//    - Add flake resource loading via use_resource + fetch_flakes()
//    - Handle loading/error states with appropriate UI
//    - Map system_count from i64 to i32 for display
//
// 2. Replace static commit samples with FlakeTimeline API data
//    - Use fetch_flake_timelines_for_ids([flake.id])
//    - Map FlakeCommit to commit list structure
//    - Extract hash (first 7 chars), message, author, committed_at
//
// 3. Replace static file samples with fetch_commit_diff()
//    - Call fetch_commit_diff(flake_id, commit_hash)
//    - Parse unified diff to extract file list with add/del stats
//    - Use existing diff parsing logic in DiffModalNew
//
// 4. Wire sync actions
//    - "Sync all" button → request_sync_all_flakes()
//    - Per-flake sync button → request_sync_flake(id)
//    - Show mutation response feedback
//
// 5. Wire deployment/build status
//    - Map FlakeCommit.build_status to PipelinePillNew
//    - Map system deployment counts to RolloutPillNew
//    - Use FlakeCommit.evaluation_status for eval pill
//
// 6. Add real-time updates
//    - Subscribe to flake sync WebSocket events
//    - Update commit timeline when new commits arrive
//    - Refresh flake list on sync completion
//
/// FlakesListViewNew - Pixel-perfect rebuild matching JSX design mockup.
/// Uses live API data for flakes, timelines, and commit diffs.
#[component]
pub fn FlakesListViewNew() -> Element {
    // Auth state for gating admin-only mutation controls
    let app_state = use_context::<Signal<AppState>>();
    let is_admin_user = auth::is_admin(&app_state.read().auth);
    
    let mut view_mode = use_signal(|| "table");
    let mut search_query = use_signal(String::new);
    let mut selected_flake = use_signal(|| None::<MockFlakeItem>);
    let mut action_notice = use_signal(|| None::<String>);
    let mut reload_nonce = use_signal(|| 0u64);
    let mut show_add_form = use_signal(|| false);
    let mut add_error = use_signal(|| None::<String>);
    let mut editing_flake = use_signal(|| None::<EditFlakeDraft>);
    let mut edit_error = use_signal(|| None::<String>);
    let mut pending_remove_new = use_signal(|| None::<MockFlakeItem>);
    let mut draft = use_signal(|| NewFlakeDraft {
        name: String::new(),
        repo_url: String::new(),
        branch: String::new(),
        build_scope: "cf_systems_only".to_string(),
        credential_type: "none".to_string(),
        credential_username: String::new(),
        credential_secret: String::new(),
        credential_ssh_username: String::new(),
    });
    let mut rewrite_prompt = use_signal(|| None::<(i32, String, String)>);

    let flakes_resource = use_resource(move || {
        let _nonce = *reload_nonce.read();
        async move { fetch_flakes().await }
    });
    let timelines_resource = use_resource(move || {
        let _nonce = *reload_nonce.read();
        async move { fetch_flake_timelines().await }
    });
    // Fetch environments for the edit dialog dropdown
    let environments_resource = use_resource(|| async { fetch_environments().await });
    let db_environments: Vec<EnvironmentSummary> = match environments_resource.read().as_ref() {
        Some(Ok(envs)) => envs.clone(),
        _ => Vec::new(),
    };

    let (raw_flakes, load_error, loading) = match flakes_resource.read().as_ref() {
        Some(Ok(items)) => (items.clone(), None, false),
        Some(Err(err)) => (Vec::new(), Some(err.to_string()), false),
        None => (Vec::new(), None, true),
    };
    let timeline_items = match timelines_resource.read().as_ref() {
        Some(Ok(items)) => items.clone(),
        _ => Vec::new(),
    };
    let mut commit_map: HashMap<i32, Vec<MockCommitItem>> = HashMap::new();
    for timeline in &timeline_items {
        commit_map.insert(
            timeline.flake_id,
            map_timeline_commits_to_view(&timeline.commits),
        );
    }
    let all_flakes: Vec<MockFlakeItem> = raw_flakes
        .iter()
        .map(|item| {
            let mut mapped = map_registry_flake_to_view(item);
            if let Some(commits) = commit_map.get(&item.id) {
                if let Some(latest) = commits.first() {
                    mapped.latest_commit = latest.sha.clone();
                    mapped.latest_message = latest.msg.clone();
                    mapped.latest_author = latest.author.clone();
                    mapped.last_sync_at = latest.at.clone();
                    mapped.total_commits = commits.len() as i32;
                    mapped.status = if latest.eval_status.as_deref() == Some("failed")
                        || latest.build_status.as_deref() == Some("failed")
                    {
                        "error".to_string()
                    } else if matches!(latest.build_status.as_deref(), Some("building") | Some("pending")) {
                        "syncing".to_string()
                    } else {
                        "synced".to_string()
                    };
                }
            }
            mapped
        })
        .collect();
    
    // Convert raw_flakes to FlakeListItem for duplicate validation
    let existing_flakes_for_validation: Vec<FlakeListItem> = raw_flakes
        .iter()
        .cloned()
        .map(FlakeListItem::from_registry)
        .collect();

    let q = search_query.read().to_lowercase();
    let filtered_flakes: Vec<MockFlakeItem> = if q.trim().is_empty() {
        all_flakes.clone()
    } else {
        all_flakes
            .iter()
            .filter(|flake| {
                flake.name.to_lowercase().contains(&q)
                    || flake.url.to_lowercase().contains(&q)
                    || flake.branch.to_lowercase().contains(&q)
                    || flake.description.to_lowercase().contains(&q)
            })
            .cloned()
            .collect()
    };

    let flake_count = filtered_flakes.len();
    let total_systems: i32 = filtered_flakes.iter().map(|f| f.system_count).sum();
    let synced_count = filtered_flakes
        .iter()
        .filter(|f| f.status == "synced")
        .count();
    let selected_flake_value = selected_flake.read().clone();
    let selected_flake_for_timeline = selected_flake.clone();
    // Use extended commit limit (200) for the tray view
    let selected_timeline_resource = use_resource(move || {
        let flake_id = selected_flake_for_timeline.read().as_ref().map(|f| f.id);
        let _nonce = *reload_nonce.read();
        async move {
            if let Some(id) = flake_id {
                // Use the tray-specific fetch with higher commit limit
                match fetch_flake_timeline_for_tray(id).await {
                    Ok(items) => {
                        let has_selected = items.iter().any(|timeline| timeline.flake_id == id);
                        if has_selected {
                            Ok(items)
                        } else {
                            // Fallback: use standard timelines fetch
                            fetch_flake_timelines().await
                        }
                    }
                    Err(_) => fetch_flake_timelines().await,
                }
            } else {
                Ok(Vec::new())
            }
        }
    });

    {
        let all_flakes = all_flakes.clone();
        let mut selected_flake = selected_flake.clone();
        use_effect(move || {
            let current = selected_flake.read().clone();
            if let Some(active) = current {
                let still_exists = all_flakes.iter().any(|flake| flake.id == active.id);
                if !still_exists {
                    selected_flake.set(None);
                }
            }
        });
    }
    
    rsx! {
        // JSX: <div style={{ display:"flex", flexDirection:"column", gap:16 }}>
        div { style: "display: flex; flex-direction: column; gap: 16px;",
            
            // Page head - JSX lines 24-39
            div { class: "page-head",
                div {
                    h1 { class: "page-title", "Flakes" }
                    p { class: "page-subtitle",
                        "{flake_count} tracked · {total_systems} systems · {synced_count} synced"
                    }
                }
                // Admin-only mutation controls: Sync all, Add flake
                if is_admin_user {
                    div { style: "display: flex; gap: 8px;",
                        button { 
                            class: "btn btn-ghost focus-ring",
                            onclick: move |_| {
                                let mut reload_nonce = reload_nonce.clone();
                                spawn(async move {
                                    let result = request_sync_all_flakes().await;
                                    match result {
                                        Ok(_) => {
                                            action_notice.set(Some("Sync requested for all flakes".to_string()));
                                            let next = *reload_nonce.read() + 1;
                                            reload_nonce.set(next);
                                        }
                                        Err(err) => action_notice.set(Some(format!("Sync all failed: {err}"))),
                                    }
                                });
                            },
                            // Inline sync icon SVG
                            svg {
                                width: "14",
                                height: "14",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                style: "display: inline-block; vertical-align: middle; margin-right: 6px;",
                                path { d: "M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2" }
                            }
                            " Sync all"
                        }
                        button { 
                            class: "btn btn-primary focus-ring",
                            onclick: move |_| {
                                show_add_form.set(true);
                                add_error.set(None);
                            },
                            // Inline plus icon SVG
                            svg {
                                width: "14",
                                height: "14",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                style: "display: inline-block; vertical-align: middle; margin-right: 6px;",
                                path { d: "M5 12h14M12 5v14" }
                            }
                            " Add flake"
                        }
                    }
                }
            }
            
            // Filter bar - JSX lines 42-52
            div { class: "filterbar",
                div { class: "filter-search",
                    // Inline search icon SVG
                    svg {
                        width: "16",
                        height: "16",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        circle { cx: "11", cy: "11", r: "8" }
                        path { d: "m21 21-4.3-4.3" }
                    }
                    input {
                        class: "input focus-ring",
                        placeholder: "Search flakes…",
                        value: "{search_query}",
                        oninput: move |evt| search_query.set(evt.value())
                    }
                }
                div { class: "seg",
                    button {
                        class: if *view_mode.read() == "table" { "active" } else { "" },
                        onclick: move |_| view_mode.set("table"),
                        // Inline rows icon SVG
                        svg {
                            width: "12",
                            height: "12",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            style: "display: inline-block; vertical-align: middle; margin-right: 6px;",
                            line { x1: "3", x2: "21", y1: "6", y2: "6" }
                            line { x1: "3", x2: "21", y1: "12", y2: "12" }
                            line { x1: "3", x2: "21", y1: "18", y2: "18" }
                        }
                        " Table"
                    }
                    button {
                        class: if *view_mode.read() == "cards" { "active" } else { "" },
                        onclick: move |_| view_mode.set("cards"),
                        // Inline grid icon SVG  
                        svg {
                            width: "12",
                            height: "12",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            style: "display: inline-block; vertical-align: middle; margin-right: 6px;",
                            rect { width: "7", height: "7", x: "3", y: "3", rx: "1" }
                            rect { width: "7", height: "7", x: "14", y: "3", rx: "1" }
                            rect { width: "7", height: "7", x: "14", y: "14", rx: "1" }
                            rect { width: "7", height: "7", x: "3", y: "14", rx: "1" }
                        }
                        " Cards"
                    }
                }
                span { class: "filter-count", "{flake_count} flakes" }
            }

            if let Some(msg) = action_notice.read().as_ref() {
                div { class: "card", style: "padding: 10px 14px; color: var(--cf-text-secondary);", "{msg}" }
            }

            // Add flake form (admin-only, guarded by show_add_form which is only set by admin button)
            if is_admin_user && *show_add_form.read() {
                AddFlakeForm {
                    draft: draft,
                    error: add_error,
                    show_onboarding_callouts: false,
                    on_cancel: move |_| {
                        show_add_form.set(false);
                        add_error.set(None);
                    },
                    on_submit: {
                        let existing_for_validation = existing_flakes_for_validation.clone();
                        move |_| {
                            let next = draft.read().clone();
                            if let Err(err) = validate_new_flake(&next, &existing_for_validation) {
                                add_error.set(Some(err));
                                return;
                            }

                            let mut draft = draft.clone();
                            let mut add_error = add_error.clone();
                            let mut show_add_form = show_add_form.clone();
                            let mut reload_nonce = reload_nonce.clone();
                            spawn(async move {
                                let request = CreateFlakeRequest {
                                    name: next.name.trim().to_string(),
                                    repo_url: next.repo_url.trim().to_string(),
                                    branch: normalize_optional_branch(&next.branch),
                                    build_scope: Some(next.build_scope.clone()),
                                };

                                match create_flake(&request).await {
                                    Ok(created) => {
                                        if let Err(error) = save_flake_credentials(created.id, &next).await {
                                            add_error.set(Some(error));
                                            return;
                                        }
                                        draft.set(NewFlakeDraft {
                                            name: String::new(),
                                            repo_url: String::new(),
                                            branch: String::new(),
                                            build_scope: "cf_systems_only".to_string(),
                                            credential_type: "none".to_string(),
                                            credential_username: String::new(),
                                            credential_secret: String::new(),
                                            credential_ssh_username: String::new(),
                                        });
                                        add_error.set(None);
                                        show_add_form.set(false);
                                        let next_nonce = *reload_nonce.read() + 1;
                                        reload_nonce.set(next_nonce);
                                    }
                                    Err(error) => add_error.set(Some(error.to_string())),
                                }
                            });
                        }
                    },
                }
            }
            
            if loading {
                div { class: "card", style: "padding: 18px; color: var(--cf-text-secondary);",
                    "Loading flakes..."
                }
            } else if let Some(error) = load_error {
                div { class: "card", style: "padding: 18px; border-color: rgba(248,113,113,0.35);",
                    div { class: "sd-callout sd-callout-danger",
                        div { style: "font-size: 12px;",
                            "Failed to load flakes: {error}"
                        }
                    }
                }
            } else {
                // Table or Cards view based on mode
                {
                    let mode: &str = &view_mode.read();
                    let selected_id = selected_flake.read().as_ref().map(|f| f.id);
                    
                    if mode == "table" {
                        rsx! { FlakeTableNew { flakes: filtered_flakes.clone(), selected_id, is_admin: is_admin_user, on_select: move |f| selected_flake.set(Some(f)), on_sync: move |flake_id| {
                            let mut reload_nonce = reload_nonce.clone();
                            spawn(async move {
                                let result = request_sync_flake(flake_id).await;
                                match result {
                                    Ok(_) => {
                                        action_notice.set(Some("Sync requested".to_string()));
                                        let next = *reload_nonce.read() + 1;
                                        reload_nonce.set(next);
                                    }
                                    Err(err) => {
                                        if let Some((id, detail)) =
                                            extract_history_rewrite_conflict(&err, Some(flake_id))
                                        {
                                            rewrite_prompt.set(Some((id, format!("flake #{id}"), detail)));
                                            action_notice.set(Some("Sync blocked: git history rewrite detected. Review and accept rewrite to continue.".to_string()));
                                        } else {
                                            action_notice.set(Some(format!("Sync failed: {err}")));
                                        }
                                    }
                                }
                            });
                        } } }
                    } else {
                        rsx! { FlakeCardsNew { flakes: filtered_flakes.clone(), selected_id, is_admin: is_admin_user, on_select: move |f| selected_flake.set(Some(f)), on_sync: move |flake_id| {
                            let mut reload_nonce = reload_nonce.clone();
                            spawn(async move {
                                let result = request_sync_flake(flake_id).await;
                                match result {
                                    Ok(_) => {
                                        action_notice.set(Some("Sync requested".to_string()));
                                        let next = *reload_nonce.read() + 1;
                                        reload_nonce.set(next);
                                    }
                                    Err(err) => {
                                        if let Some((id, detail)) =
                                            extract_history_rewrite_conflict(&err, Some(flake_id))
                                        {
                                            rewrite_prompt.set(Some((id, format!("flake #{id}"), detail)));
                                            action_notice.set(Some("Sync blocked: git history rewrite detected. Review and accept rewrite to continue.".to_string()));
                                        } else {
                                            action_notice.set(Some(format!("Sync failed: {err}")));
                                        }
                                    }
                                }
                            });
                        } } }
                    }
                }
            }
            
            // Side tray (if flake selected)
            if let Some(flake) = selected_flake_value {
                {
                    let selected_direct_commits = match selected_timeline_resource.read().as_ref() {
                        Some(Ok(items)) => items
                            .iter()
                            .find(|timeline| timeline.flake_id == flake.id)
                            .map(|timeline| map_timeline_commits_to_view(&timeline.commits))
                            .unwrap_or_default(),
                        _ => Vec::new(),
                    };
                    let selected_direct_loading =
                        matches!(selected_timeline_resource.read().as_ref(), None);
                    let selected_direct_error = match selected_timeline_resource.read().as_ref() {
                        Some(Err(err)) => Some(err.to_string()),
                        _ => None,
                    };
                    let tray_commits = selected_direct_commits;
                    let tray_commits_loading = selected_direct_loading;
                    let tray_commits_error = if tray_commits.is_empty() {
                        selected_direct_error
                    } else {
                        None
                    };
                    let all_flakes_for_edit = all_flakes.clone();

                    rsx! {
                        FlakeTrayNew {
                            commits: tray_commits,
                            commits_loading: tray_commits_loading,
                            commits_error: tray_commits_error,
                            notice: action_notice.read().clone(),
                            is_admin: is_admin_user,
                            flake,
                            on_edit: move |flake_id| {
                                if let Some(current) = all_flakes_for_edit.iter().find(|item| item.id == flake_id) {
                                    let base_draft = EditFlakeDraft {
                                        id: current.id,
                                        name: current.name.clone(),
                                        repo_url: current.url.clone(),
                                        branch: current.branch.clone(),
                                        environment: current.environment.clone(),
                                        description: current.description.clone(),
                                        build_scope: current.build_scope.clone(),
                                        credential_type: "none".to_string(),
                                        credential_username: String::new(),
                                        credential_secret: String::new(),
                                        credential_ssh_username: String::new(),
                                        has_existing_secret: false,
                                    };
                                    edit_error.set(None);

                                    let mut editing_flake = editing_flake.clone();
                                    let mut edit_error = edit_error.clone();
                                    spawn(async move {
                                        match fetch_flake_credentials(flake_id).await {
                                            Ok(credentials) => {
                                                let mut draft = base_draft.clone();
                                                draft.credential_type =
                                                    normalize_credential_type(&credentials.auth_type);
                                                draft.credential_username =
                                                    credentials.username.unwrap_or_default();
                                                draft.credential_ssh_username =
                                                    credentials.ssh_username.unwrap_or_default();
                                                draft.has_existing_secret = credentials.has_secret;
                                                editing_flake.set(Some(draft));
                                            }
                                            Err(error) => {
                                                editing_flake.set(Some(base_draft));
                                                edit_error
                                                    .set(Some(format!("Failed to load credentials: {error}")));
                                            }
                                        }
                                    });
                                }
                            },
                            on_sync: move |flake_id| {
                                let mut reload_nonce = reload_nonce.clone();
                                spawn(async move {
                                    let result = request_sync_flake(flake_id).await;
                                    match result {
                                        Ok(_) => {
                                            action_notice.set(Some("Sync requested".to_string()));
                                            let next = *reload_nonce.read() + 1;
                                            reload_nonce.set(next);

                                            // Sync is asynchronous server-side; poll timeline refresh shortly after request.
                                            #[cfg(target_arch = "wasm32")]
                                            {
                                                use gloo_timers::future::TimeoutFuture;
                                                TimeoutFuture::new(1200).await;
                                                let next = *reload_nonce.read() + 1;
                                                reload_nonce.set(next);

                                                TimeoutFuture::new(2200).await;
                                                let next = *reload_nonce.read() + 1;
                                                reload_nonce.set(next);
                                            }
                                        }
                                        Err(err) => {
                                            if let Some((id, detail)) =
                                                extract_history_rewrite_conflict(&err, Some(flake_id))
                                            {
                                                rewrite_prompt.set(Some((id, format!("flake #{id}"), detail)));
                                                action_notice.set(Some("Sync blocked: git history rewrite detected. Review and accept rewrite to continue.".to_string()));
                                            } else {
                                                action_notice
                                                    .set(Some(format!("Sync failed: {err}")))
                                            }
                                        }
                                    }
                                });
                            },
                            on_history_rewrite_conflict: move |(flake_id, detail)| {
                                rewrite_prompt.set(Some((flake_id, format!("flake #{flake_id}"), detail)));
                                action_notice.set(Some("Sync blocked: git history rewrite detected. Review and accept rewrite to continue.".to_string()));
                            },
                            on_close: move |_| selected_flake.set(None)
                        }
                    }
                }
            }

            if let Some((flake_id, flake_name, detail)) = rewrite_prompt.read().clone() {
                HistoryRewriteDialog {
                    flake_name,
                    detail,
                    on_cancel: move |_| rewrite_prompt.set(None),
                    on_accept: move |_| {
                        let mut rewrite_prompt = rewrite_prompt.clone();
                        let mut action_notice = action_notice.clone();
                        let mut reload_nonce = reload_nonce.clone();
                        spawn(async move {
                            match accept_flake_history_rewrite(flake_id).await {
                                Ok(response) => {
                                    rewrite_prompt.set(None);
                                    match request_sync_flake(flake_id).await {
                                        Ok(_) => {
                                            action_notice.set(Some(response.message));
                                            let next = *reload_nonce.read() + 1;
                                            reload_nonce.set(next);
                                        }
                                        Err(err) => {
                                            action_notice.set(Some(format!(
                                                "History rewrite accepted, but sync failed: {err}"
                                            )));
                                        }
                                    }
                                }
                                Err(err) => action_notice
                                    .set(Some(format!("Failed to accept rewrite: {err}"))),
                            }
                        });
                    }
                }
            }

            if let Some(flake) = pending_remove_new.read().clone() {
                RemoveFlakeDialog {
                    flake_name: flake.name.clone(),
                    system_count: flake.system_count.max(0) as usize,
                    on_cancel: move |_| pending_remove_new.set(None),
                    on_confirm: move |(hard, cascade)| {
                        let remove_id = flake.id;
                        let mut pending_remove_new = pending_remove_new.clone();
                        let mut action_notice = action_notice.clone();
                        let mut reload_nonce = reload_nonce.clone();
                        let mut selected_flake = selected_flake.clone();
                        spawn(async move {
                            match delete_flake(remove_id, hard, cascade).await {
                                Ok(()) => {
                                    pending_remove_new.set(None);
                                    if selected_flake.read().as_ref().is_some_and(|item| item.id == remove_id)
                                    {
                                        selected_flake.set(None);
                                    }
                                    action_notice.set(Some("Flake removed from registry".to_string()));
                                    let next = *reload_nonce.read() + 1;
                                    reload_nonce.set(next);
                                }
                                Err(error) => {
                                    action_notice.set(Some(error.to_string()));
                                }
                            }
                        });
                    }
                }
            }

            if let Some(editing) = editing_flake.read().clone() {
                EditFlakeDialog {
                    draft: editing,
                    error: edit_error,
                    environments: db_environments.clone(),
                    on_remove: {
                        let all_flakes_for_remove = all_flakes.clone();
                        move |flake_id| {
                            if let Some(target) = all_flakes_for_remove.iter().find(|item| item.id == flake_id).cloned() {
                                pending_remove_new.set(Some(target));
                                editing_flake.set(None);
                                edit_error.set(None);
                            }
                        }
                    },
                    on_cancel: move |_| {
                        editing_flake.set(None);
                        edit_error.set(None);
                    },
                    on_change: move |next| editing_flake.set(Some(next)),
                    on_submit: move |_| {
                        let Some(next) = editing_flake.read().clone() else {
                            return;
                        };

                        let mut editing_flake = editing_flake.clone();
                        let mut edit_error = edit_error.clone();
                        let mut action_notice = action_notice.clone();
                        let mut reload_nonce = reload_nonce.clone();
                        spawn(async move {
                            let request = UpdateFlakeRequest {
                                name: next.name.trim().to_string(),
                                repo_url: next.repo_url.trim().to_string(),
                                branch: normalize_optional_branch(&next.branch),
                                build_scope: Some(next.build_scope.clone()),
                            };

                            match update_flake(next.id, &request).await {
                                Ok(updated) => {
                                    if let Err(error) = save_flake_credentials(updated.id, &next).await {
                                        edit_error.set(Some(error));
                                        return;
                                    }
                                    editing_flake.set(None);
                                    edit_error.set(None);
                                    action_notice.set(Some("Flake updated".to_string()));
                                    let next_reload = *reload_nonce.read() + 1;
                                    reload_nonce.set(next_reload);
                                }
                                Err(error) => {
                                    edit_error.set(Some(error.to_string()));
                                }
                            }
                        });
                    }
                }
            }
        }
    }
}

// ============================================================================
// Phase 2: Mock Data Structures for Table/Cards
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
struct MockFlakeItem {
    id: i32,
    name: String,
    description: String,
    status: String,
    url: String,
    branch: String,
    build_scope: String,
    system_count: i32,
    latest_commit: String,
    latest_message: String,
    latest_author: String,
    last_sync_at: String,
    environment: String,
    error_msg: Option<String>,
    total_commits: i32,
}

fn map_registry_flake_to_view(item: &FlakeRegistryItem) -> MockFlakeItem {
    let build_scope_label = if item.build_scope.trim().is_empty() {
        "default"
    } else {
        item.build_scope.trim()
    };

    MockFlakeItem {
        id: item.id,
        name: item.name.clone(),
        description: format!("Build scope: {build_scope_label}"),
        status: "synced".to_string(),
        url: item.repo_url.clone(),
        branch: item.branch.clone(),
        build_scope: item.build_scope.clone(),
        system_count: item.system_count as i32,
        latest_commit: "—".to_string(),
        latest_message: "No commits yet".to_string(),
        latest_author: "—".to_string(),
        last_sync_at: "Not synced yet".to_string(),
        environment: build_scope_label.to_string(),
        error_msg: None,
        total_commits: 0,
    }
}

fn relative_time_label(ts: DateTime<Utc>) -> String {
    let now = Utc::now();
    let delta = now.signed_duration_since(ts);
    if delta < Duration::minutes(1) {
        "now".to_string()
    } else if delta < Duration::hours(1) {
        format!("{}m ago", delta.num_minutes())
    } else if delta < Duration::days(1) {
        format!("{}h ago", delta.num_hours())
    } else if delta < Duration::days(7) {
        format!("{}d ago", delta.num_days())
    } else {
        format!("{}w ago", (delta.num_days() / 7).max(1))
    }
}

fn build_status_token(status: Option<ApiBuildStatus>) -> Option<String> {
    status.map(|s| match s {
        ApiBuildStatus::Idle => "pending".to_string(),
        ApiBuildStatus::Queued => "pending".to_string(),
        ApiBuildStatus::Building => "building".to_string(),
        ApiBuildStatus::Cancelling => "building".to_string(),
        ApiBuildStatus::Complete => "complete".to_string(),
        ApiBuildStatus::Failed => "failed".to_string(),
        ApiBuildStatus::Cancelled => "failed".to_string(),
    })
}

fn map_timeline_commits_to_view(commits: &[crate::api::models::FlakeCommit]) -> Vec<MockCommitItem> {
    let mut mapped = commits
        .iter()
        .map(|c| {
            let short = c.hash.chars().take(7).collect::<String>();
            MockCommitItem {
                sha: short,
                full_hash: c.hash.clone(),
                msg: c.message.clone(),
                author: c.author.clone(),
                at: relative_time_label(c.committed_at),
                committed_at: c.committed_at,
                files: c.system_paths.len() as i32,
                add: 0,
                del: 0,
                eval_status: c.evaluation_status.clone(),
                build_status: build_status_token(c.build_status),
                rollout_on: c.system_count as i32,
                rollout_total: c.systems.len() as i32,
            }
        })
        .collect::<Vec<_>>();

    mapped.sort_by(|a, b| b.committed_at.cmp(&a.committed_at));
    mapped
}

fn map_diff_to_file_cards(diff: &str) -> Vec<MockFileItem> {
    let parsed_cards = parse_unified_diff(diff)
        .into_iter()
        .filter(|file| file.old_path != "(unknown)" || file.new_path != "(unknown)")
        .map(|file| {
            let (add, del) = diff_file_stats(&file);
            MockFileItem {
                name: diff_file_label(&file),
                add: add as i32,
                del: del as i32,
            }
        })
        .collect::<Vec<_>>();

    if parsed_cards.len() > 1 {
        return parsed_cards;
    }

    let mut fallback_cards = Vec::new();
    let mut current_old: Option<String> = None;
    let mut current_new: Option<String> = None;
    let mut add = 0i32;
    let mut del = 0i32;

    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("--- ") {
            if let (Some(old_path), Some(new_path)) = (current_old.take(), current_new.take()) {
                let parsed = ParsedDiffFile {
                    old_path: old_path.trim_start_matches("a/").to_string(),
                    new_path: new_path.trim_start_matches("b/").to_string(),
                    language: "text",
                    lines: Vec::new(),
                };
                fallback_cards.push(MockFileItem {
                    name: diff_file_label(&parsed),
                    add,
                    del,
                });
                add = 0;
                del = 0;
            }
            current_old = Some(path.trim().to_string());
            continue;
        }

        if let Some(path) = line.strip_prefix("+++ ") {
            current_new = Some(path.trim().to_string());
            continue;
        }

        if line.starts_with('+') && !line.starts_with("+++") {
            add += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            del += 1;
        }
    }

    if let (Some(old_path), Some(new_path)) = (current_old.take(), current_new.take()) {
        let parsed = ParsedDiffFile {
            old_path: old_path.trim_start_matches("a/").to_string(),
            new_path: new_path.trim_start_matches("b/").to_string(),
            language: "text",
            lines: Vec::new(),
        };
        fallback_cards.push(MockFileItem {
            name: diff_file_label(&parsed),
            add,
            del,
        });
    }

    if fallback_cards.len() > parsed_cards.len() {
        fallback_cards
    } else {
        parsed_cards
    }
}

/// Construct a URL to view a file at a specific commit on the git remote.
/// Supports GitLab, GitHub, and generic git URLs.
fn construct_git_file_url(repo_url: &str, commit_hash: &str, file_path: &str) -> String {
    // Normalize the repo URL (remove .git suffix, convert SSH to HTTPS)
    let normalized = repo_url
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git");
    
    // Convert SSH URLs to HTTPS
    let https_url = if normalized.starts_with("git@") {
        // git@gitlab.com:owner/repo -> https://gitlab.com/owner/repo
        normalized
            .replacen("git@", "https://", 1)
            .replacen(':', "/", 1)
    } else {
        normalized.to_string()
    };
    
    // Determine the URL pattern based on the host
    if https_url.contains("gitlab.com") || https_url.contains("gitlab.") {
        // GitLab: https://gitlab.com/owner/repo/-/blob/{commit}/{path}
        format!("{}/-/blob/{}/{}", https_url, commit_hash, file_path)
    } else if https_url.contains("github.com") {
        // GitHub: https://github.com/owner/repo/blob/{commit}/{path}
        format!("{}/blob/{}/{}", https_url, commit_hash, file_path)
    } else if https_url.contains("bitbucket.org") {
        // Bitbucket: https://bitbucket.org/owner/repo/src/{commit}/{path}
        format!("{}/src/{}/{}", https_url, commit_hash, file_path)
    } else {
        // Generic fallback - try GitLab-style URL
        format!("{}/-/blob/{}/{}", https_url, commit_hash, file_path)
    }
}

fn extract_diff_block_for_file_label(diff: &str, target_label: &str) -> Option<String> {
    let mut current_block: Vec<String> = Vec::new();

    for line in diff.lines() {
        if line.starts_with("diff --git ") && !current_block.is_empty() {
            let parsed = parse_diff_file_block(&current_block);
            if diff_file_label(&parsed) == target_label {
                return Some(current_block.join("\n"));
            }
            current_block.clear();
        }
        current_block.push(line.to_string());
    }

    if !current_block.is_empty() {
        let parsed = parse_diff_file_block(&current_block);
        if diff_file_label(&parsed) == target_label {
            return Some(current_block.join("\n"));
        }
    }

    None
}

#[allow(dead_code)]
fn mock_flakes_data() -> Vec<MockFlakeItem> {
    vec![
        MockFlakeItem {
            id: 1,
            name: "infrastructure".to_string(),
            description: "Core infrastructure configs".to_string(),
            status: "synced".to_string(),
            url: "git@gitlab.com:org/infra.git".to_string(),
            branch: "main".to_string(),
            build_scope: "cf_systems_only".to_string(),
            system_count: 12,
            latest_commit: "a3f4b2c".to_string(),
            latest_message: "feat: Add monitoring dashboards".to_string(),
            latest_author: "jdoe".to_string(),
            last_sync_at: "2m ago".to_string(),
            environment: "production".to_string(),
            error_msg: None,
            total_commits: 156,
        },
        MockFlakeItem {
            id: 2,
            name: "applications".to_string(),
            description: "Application deployments".to_string(),
            status: "syncing".to_string(),
            url: "git@gitlab.com:org/apps.git".to_string(),
            branch: "develop".to_string(),
            build_scope: "all_configs".to_string(),
            system_count: 8,
            latest_commit: "b8d1e9a".to_string(),
            latest_message: "fix: Update container versions".to_string(),
            latest_author: "asmith".to_string(),
            last_sync_at: "5m ago".to_string(),
            environment: "staging".to_string(),
            error_msg: None,
            total_commits: 89,
        },
        MockFlakeItem {
            id: 3,
            name: "edge-nodes".to_string(),
            description: "Edge computing nodes".to_string(),
            status: "error".to_string(),
            url: "git@gitlab.com:org/edge.git".to_string(),
            branch: "main".to_string(),
            build_scope: "cf_systems_only".to_string(),
            system_count: 24,
            latest_commit: "c2f7d4e".to_string(),
            latest_message: "refactor: Optimize network config".to_string(),
            latest_author: "mlee".to_string(),
            last_sync_at: "1h ago".to_string(),
            environment: "edge".to_string(),
            error_msg: Some("Failed to fetch: connection timeout".to_string()),
            total_commits: 234,
        },
    ]
}

// ============================================================================
// Mock commit data structures for FlakeTray
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
struct MockCommitItem {
    sha: String,
    full_hash: String,
    msg: String,
    author: String,
    at: String,
    committed_at: DateTime<Utc>,
    files: i32,
    add: i32,
    del: i32,
    eval_status: Option<String>,
    build_status: Option<String>,
    rollout_on: i32,
    rollout_total: i32,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
struct MockPipelineStatus {
    eval: Option<String>,
    build: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
struct MockFileItem {
    name: String,
    add: i32,
    del: i32,
}

#[allow(dead_code)]
fn mock_commits_for_flake(flake_id: i32) -> Vec<MockCommitItem> {
    match flake_id {
        1 => vec![
            MockCommitItem {
                sha: "a3f8c12".to_string(),
                full_hash: "a3f8c12".to_string(),
                msg: "stig: enforce audit rules for sudo".to_string(),
                author: "mreyes".to_string(),
                at: "2h ago".to_string(),
                committed_at: Utc::now() - Duration::hours(2),
                files: 3,
                add: 28,
                del: 4,
                eval_status: Some("complete".to_string()),
                build_status: Some("cache-pushed".to_string()),
                rollout_on: 8,
                rollout_total: 12,
            },
            MockCommitItem {
                sha: "f1d9022".to_string(),
                full_hash: "f1d9022".to_string(),
                msg: "cve: patch openssl to 3.3.2".to_string(),
                author: "ops-bot".to_string(),
                at: "1d ago".to_string(),
                committed_at: Utc::now() - Duration::days(1),
                files: 2,
                add: 12,
                del: 8,
                eval_status: Some("complete".to_string()),
                build_status: Some("building".to_string()),
                rollout_on: 7,
                rollout_total: 12,
            },
            MockCommitItem {
                sha: "8c4b311".to_string(),
                full_hash: "8c4b311".to_string(),
                msg: "atlas-02: add prometheus node exporter".to_string(),
                author: "dchen".to_string(),
                at: "2d ago".to_string(),
                committed_at: Utc::now() - Duration::days(2),
                files: 1,
                add: 14,
                del: 0,
                eval_status: Some("complete".to_string()),
                build_status: Some("complete".to_string()),
                rollout_on: 6,
                rollout_total: 12,
            },
            MockCommitItem {
                sha: "77aef00".to_string(),
                full_hash: "77aef00".to_string(),
                msg: "bump nixpkgs to 24.11.20260401".to_string(),
                author: "ops-bot".to_string(),
                at: "3d ago".to_string(),
                committed_at: Utc::now() - Duration::days(3),
                files: 1,
                add: 2,
                del: 2,
                eval_status: Some("failed".to_string()),
                build_status: None,
                rollout_on: 6,
                rollout_total: 12,
            },
            MockCommitItem {
                sha: "3c12889".to_string(),
                full_hash: "3c12889".to_string(),
                msg: "orion-db: add pgbackup systemd timer".to_string(),
                author: "jpark".to_string(),
                at: "5d ago".to_string(),
                committed_at: Utc::now() - Duration::days(5),
                files: 2,
                add: 31,
                del: 0,
                eval_status: Some("pending".to_string()),
                build_status: None,
                rollout_on: 5,
                rollout_total: 12,
            },
            MockCommitItem {
                sha: "a22fc08".to_string(),
                full_hash: "a22fc08".to_string(),
                msg: "harden sshd: disable password auth".to_string(),
                author: "mreyes".to_string(),
                at: "1w ago".to_string(),
                committed_at: Utc::now() - Duration::weeks(1),
                files: 1,
                add: 6,
                del: 3,
                eval_status: Some("complete".to_string()),
                build_status: Some("complete".to_string()),
                rollout_on: 4,
                rollout_total: 12,
            },
        ],
        _ => vec![
            MockCommitItem {
                sha: "abc1234".to_string(),
                full_hash: "abc1234".to_string(),
                msg: "Initial commit".to_string(),
                author: "dev".to_string(),
                at: "1d ago".to_string(),
                committed_at: Utc::now() - Duration::days(1),
                files: 1,
                add: 10,
                del: 0,
                eval_status: Some("complete".to_string()),
                build_status: Some("complete".to_string()),
                rollout_on: 1,
                rollout_total: 1,
            },
        ],
    }
}

#[allow(dead_code)]
fn mock_pipeline_status_for_index(index: usize) -> MockPipelineStatus {
    let statuses = vec![
        MockPipelineStatus {
            eval: Some("complete".to_string()),
            build: Some("cache-pushed".to_string()),
        },
        MockPipelineStatus {
            eval: Some("complete".to_string()),
            build: Some("building".to_string()),
        },
        MockPipelineStatus {
            eval: Some("complete".to_string()),
            build: Some("complete".to_string()),
        },
        MockPipelineStatus {
            eval: Some("failed".to_string()),
            build: None,
        },
        MockPipelineStatus {
            eval: Some("pending".to_string()),
            build: None,
        },
    ];
    statuses[index % statuses.len()].clone()
}

#[allow(dead_code)]
fn mock_diff_for_file(file_name: &str) -> String {
    // Return a realistic unified diff format
    format!(r#"--- a/{}
+++ b/{}
@@ -14,8 +14,14 @@
 {{
   services.openssh = {{
     enable = true;
-    settings.PasswordAuthentication = true;
+    settings.PasswordAuthentication = false;
+    settings.KbdInteractiveAuthentication = false;
+    settings.PermitRootLogin = "no";
+    settings.MaxAuthTries = 3;
+    settings.ClientAliveInterval = 300;
+    settings.ClientAliveCountMax = 0;
   }};
 }}
@@ -42,12 +48,28 @@
   networking = {{
     hostName = "atlas-01";
     domain = "cf.internal";
-    firewall.allowedTCPPorts = [ 22 80 443 ];
+    firewall = {{
+      allowedTCPPorts = [ 22 80 443 9100 9090 ];
+      allowedUDPPorts = [ 51820 ];
+      logRefusedConnections = true;
+      logRefusedPackets = false;
+      extraCommands = ''
+        iptables -A INPUT -p tcp --dport 22 -m connlimit --connlimit-above 4 -j REJECT
+        iptables -A INPUT -m state --state INVALID -j DROP
+      '';
+    }};
+    nameservers = [ "10.0.0.1" "10.0.0.2" ];
+    defaultGateway = "10.0.0.1";
   }};
 }}"#, file_name, file_name)
}

#[allow(dead_code)]
fn mock_files_for_commit(sha: &str) -> Vec<MockFileItem> {
    match sha {
        "a3f8c12" => vec![
            MockFileItem { name: "modules/security/auditd.nix".to_string(), add: 18, del: 2 },
            MockFileItem { name: "modules/security/sudo.nix".to_string(), add: 8, del: 1 },
            MockFileItem { name: "hosts/atlas-01/configuration.nix".to_string(), add: 2, del: 1 },
        ],
        "f1d9022" => vec![
            MockFileItem { name: "pkgs/openssl/default.nix".to_string(), add: 10, del: 6 },
            MockFileItem { name: "flake.lock".to_string(), add: 2, del: 2 },
        ],
        "8c4b311" => vec![
            MockFileItem { name: "hosts/atlas-02/monitoring.nix".to_string(), add: 14, del: 0 },
        ],
        _ => vec![
            MockFileItem { name: "README.md".to_string(), add: 5, del: 0 },
        ],
    }
}

// ============================================================================
// Phase 2: FlakeTable Component - Matching JSX lines 451-495
// ============================================================================

#[allow(dead_code)]
#[component]
fn FlakeTableNew(
    flakes: Vec<MockFlakeItem>,
    selected_id: Option<i32>,
    is_admin: bool,
    on_select: EventHandler<MockFlakeItem>,
    on_sync: EventHandler<i32>,
) -> Element {
    rsx! {
        // JSX: <div className="card" style={{ overflow:"hidden" }}>
        div { class: "card", style: "overflow: hidden;",
            table { class: "sys-table",
                thead {
                    tr {
                        th { "Flake" }
                        th { "Status" }
                        th { "Branch" }
                        th { "Systems" }
                        th { "Latest commit" }
                        th { "Author" }
                        th { "Synced" }
                        th { style: "text-align: right;", " " }
                    }
                }
                tbody {
                    for flake in flakes {
                        {
                            let is_selected = selected_id == Some(flake.id);
                            let row_class = if is_selected { "selected" } else { "" };
                            let flake_for_select = flake.clone();
                            let flake_id_for_sync = flake.id;
                            
                            rsx! {
                                tr {
                                    key: "{flake.id}",
                                    class: "{row_class}",
                                    style: "cursor: pointer;",
                                    onclick: move |_| on_select.call(flake_for_select.clone()),
                                    
                                    // Flake name and description
                                    td {
                                        div { style: "font-weight: 600; font-size: 13px;", "{flake.name}" }
                                        div { style: "font-size: 11px; color: var(--cf-text-muted);", "{flake.description}" }
                                    }
                                    
                                    // Status chip
                                    td {
                                        FlakeSyncChipNew { status: flake.status.clone(), error_msg: flake.error_msg.clone() }
                                    }
                                    
                                    // Branch
                                    td {
                                        span { class: "chip chip-unknown", "{flake.branch}" }
                                    }
                                    
                                    // Systems count
                                    td { style: "font-size: 13px;", "{flake.system_count}" }
                                    
                                    // Latest commit
                                    td {
                                        span { 
                                            class: "mono", 
                                            style: "font-size: 12px; font-weight: 600;", 
                                            "{flake.latest_commit}" 
                                        }
                                        div { 
                                            style: "font-size: 11px; color: var(--cf-text-muted); max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                            "{flake.latest_message}"
                                        }
                                    }
                                    
                                    // Author
                                    td { 
                                        class: "mono",
                                        style: "font-size: 12px; color: var(--cf-text-secondary);",
                                        "{flake.latest_author}"
                                    }
                                    
                                    // Last synced
                                    td { style: "font-size: 12px; color: var(--cf-text-muted);", "{flake.last_sync_at}" }
                                    
                                    // Actions (admin-only)
                                    td {
                                        if is_admin {
                                            div { class: "row-actions",
                                                button {
                                                    class: "btn-icon focus-ring",
                                                    title: "Sync",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_sync.call(flake_id_for_sync);
                                                    },
                                                    // Inline sync icon
                                                    svg {
                                                        width: "14",
                                                        height: "14",
                                                        view_box: "0 0 24 24",
                                                        fill: "none",
                                                        stroke: "currentColor",
                                                        stroke_width: "2",
                                                        stroke_linecap: "round",
                                                        stroke_linejoin: "round",
                                                        path { d: "M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2" }
                                                    }
                                                }
                                                button { 
                                                    class: "btn-icon focus-ring",
                                                    title: "More",
                                                    onclick: move |evt| evt.stop_propagation(),
                                                    // Inline more icon (3 dots)
                                                    svg {
                                                        width: "14",
                                                    height: "14",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    circle { cx: "12", cy: "12", r: "1" }
                                                    circle { cx: "12", cy: "5", r: "1" }
                                                    circle { cx: "12", cy: "19", r: "1" }
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
            }
        }
    }
}

// Helper component for status chip
#[allow(dead_code)]
#[component]
fn FlakeSyncChipNew(status: String, error_msg: Option<String>) -> Element {
    // JSX: const cfg = { synced:["chip-healthy","#34d399","synced"], ... }
    let (chip_class, color, label) = match status.as_str() {
        "synced" => ("chip-healthy", "#34d399", "synced"),
        "syncing" => ("chip-info", "#60a5fa", "syncing"),
        "error" => ("chip-critical", "#f87171", "error"),
        _ => ("chip-unknown", "#6b7280", status.as_str()),
    };
    
    let title = error_msg.as_deref().unwrap_or("");
    
    rsx! {
        span { 
            class: "chip {chip_class}",
            title: "{title}",
            span { 
                class: "chip-dot",
                style: "background: {color};"
            }
            "{label}"
        }
    }
}


// ============================================================================
// Phase 2: FlakeCards Component - Matching JSX lines 498-540
// ============================================================================

#[allow(dead_code)]
#[component]
fn FlakeCardsNew(
    flakes: Vec<MockFlakeItem>,
    selected_id: Option<i32>,
    is_admin: bool,
    on_select: EventHandler<MockFlakeItem>,
    on_sync: EventHandler<i32>,
) -> Element {
    rsx! {
        // JSX: <div className="cards-grid">
        div { class: "cards-grid",
            for flake in flakes {
                {
                    let is_selected = selected_id == Some(flake.id);
                    let flake_for_select = flake.clone();
                    let flake_id_for_sync = flake.id;
                    // JSX: const statusColor = { synced:"#34d399", ... }
                    let status_color = match flake.status.as_str() {
                        "synced" => "#34d399",
                        "syncing" => "#60a5fa",
                        "error" => "#f87171",
                        _ => "#6b7280",
                    };
                    let border_style = if is_selected {
                        "border-color: var(--cf-brand-purple);"
                    } else {
                        ""
                    };
                    
                    rsx! {
                        div {
                            key: "{flake.id}",
                            class: "sys-card",
                            style: "{border_style}",
                            onclick: move |_| {
                                on_select.call(flake_for_select.clone());
                            },
                            
                            // JSX: <div className="status-rail" style={{ "--status-color": statusColor }}/>
                            div { 
                                class: "status-rail",
                                style: "--status-color: {status_color};"
                            }
                            
                            // Card header
                            div { class: "sys-card-head",
                                div { class: "sys-title",
                                    div { class: "sys-hostname",
                                        // Inline git icon
                                        svg {
                                            width: "13",
                                            height: "13",
                                            view_box: "0 0 24 24",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            style: "display: inline-block; vertical-align: middle; margin-right: 4px;",
                                            path { d: "M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4M9 18c-4.51 2-5-2-7-2" }
                                        }
                                        " {flake.name}"
                                    }
                                    div { class: "sys-fqdn", "{flake.url}" }
                                }
                                EnvBadgeNew { env: flake.environment.clone() }
                            }
                            
                            // Description (limit to 2 lines)
                            div { 
                                style: "font-size: 12px; color: var(--cf-text-secondary); display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;",
                                "{flake.description}"
                            }
                            
                            // Card body - key-value grid
                            div { class: "sys-card-body",
                                div {
                                    div { class: "sys-kv-key", "Branch" }
                                    div { class: "sys-kv-val", "{flake.branch}" }
                                }
                                div {
                                    div { class: "sys-kv-key", "Systems" }
                                    div { 
                                        class: "sys-kv-val",
                                        style: "font-family: inherit;",
                                        "{flake.system_count}"
                                    }
                                }
                                div {
                                    div { class: "sys-kv-key", "Commit" }
                                    div { class: "sys-kv-val", "{flake.latest_commit}" }
                                }
                                div {
                                    div { class: "sys-kv-key", "Synced" }
                                    div { 
                                        class: "sys-kv-val",
                                        style: "font-family: inherit;",
                                        "{flake.last_sync_at}"
                                    }
                                }
                            }
                            
                            // Error callout (if error)
                            if let Some(error_msg) = &flake.error_msg {
                                div {
                                    class: "sd-callout sd-callout-danger",
                                    style: "padding: 8px 10px;",
                                    // Inline warn icon
                                    svg {
                                        width: "12",
                                        height: "12",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        style: "display: inline-block; vertical-align: middle; margin-right: 6px;",
                                        path { d: "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" }
                                        path { d: "M12 9v4" }
                                        path { d: "M12 17h.01" }
                                    }
                                    div { style: "font-size: 11px;", "{error_msg}" }
                                }
                            }
                            
                            // Card footer
                            div { class: "sys-card-foot",
                                div { class: "chips-row",
                                    FlakeSyncChipNew { 
                                        status: flake.status.clone(),
                                        error_msg: flake.error_msg.clone()
                                    }
                                    span { 
                                        class: "chip chip-unknown",
                                        "{flake.total_commits} commits"
                                    }
                                }
                                // Admin-only sync button
                                if is_admin {
                                    button {
                                        class: "btn btn-subtle focus-ring",
                                        style: "padding: 4px 10px; font-size: 12px;",
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            on_sync.call(flake_id_for_sync);
                                        },
                                        // Inline sync icon
                                        svg {
                                            width: "12",
                                            height: "12",
                                            view_box: "0 0 24 24",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                        style: "display: inline-block; vertical-align: middle; margin-right: 6px;",
                                        path { d: "M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2" }
                                    }
                                    " Sync"
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

// Helper component for environment badge
#[allow(dead_code)]
#[component]
fn EnvBadgeNew(env: String) -> Element {
    let chip_class = match env.as_str() {
        "production" => "chip-critical",
        "staging" => "chip-warning",
        "dev" => "chip-info",
        "edge" => "chip-info",
        _ => "chip-unknown",
    };
    
    rsx! {
        span { class: "chip {chip_class}", "{env}" }
    }
}


// ============================================================================
// Phase 3: FlakeTray - Side panel with backdrop and header
// Matching JSX lines 114-134 (header structure)
// ============================================================================

#[allow(dead_code)]
#[component]
fn FlakeTrayNew(
    flake: MockFlakeItem,
    commits: Vec<MockCommitItem>,
    commits_loading: bool,
    commits_error: Option<String>,
    notice: Option<String>,
    is_admin: bool,
    on_edit: EventHandler<i32>,
    on_sync: EventHandler<i32>,
    on_history_rewrite_conflict: EventHandler<(i32, String)>,
    on_close: EventHandler<()>,
) -> Element {
    const INITIAL_VISIBLE_COMMITS: usize = 100;
    const LOAD_MORE_STEP: usize = 100;

    let mut selected_commit = use_signal(|| commits.first().cloned());
    let mut unavailable_commit_hashes = use_signal(Vec::<String>::new);
    let mut commit_query = use_signal(String::new);
    let mut visible_limit = use_signal(|| INITIAL_VISIBLE_COMMITS);
    let commits_scroll_id = format!("fl-tray-commits-{}", flake.id);
    let query = commit_query.read().trim().to_lowercase();
    let filtered_commits = if query.is_empty() {
        commits.clone()
    } else {
        commits
            .iter()
            .filter(|commit| {
                commit.sha.to_lowercase().contains(&query)
                    || commit.msg.to_lowercase().contains(&query)
                    || commit.author.to_lowercase().contains(&query)
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    let visible_commits: Vec<MockCommitItem> = filtered_commits
        .iter()
        .take((*visible_limit.read()).min(filtered_commits.len()))
        .cloned()
        .collect();
    let has_more_commits = visible_commits.len() < filtered_commits.len();
    {
        let mut visible_limit = visible_limit.clone();
        let commits_scroll_id = commits_scroll_id.clone();
        let total = filtered_commits.len();
        use_effect(move || {
            if total == 0 {
                return;
            }
            let Some(window) = window() else {
                return;
            };
            let Some(document) = window.document() else {
                return;
            };

            let commits_scroll_id_for_handler = commits_scroll_id.clone();
            let handler = Closure::<dyn FnMut()>::new(move || {
                if *visible_limit.read() >= total {
                    return;
                }
                let Some(element) = document.get_element_by_id(&commits_scroll_id_for_handler) else {
                    return;
                };
                let scroll_top = element.scroll_top();
                let client_height = element.client_height();
                let scroll_height = element.scroll_height();
                if scroll_top + client_height + 96 >= scroll_height {
                    let next = *visible_limit.read() + LOAD_MORE_STEP;
                    visible_limit.set(next.min(total));
                }
            });

            let _ = window.set_interval_with_callback_and_timeout_and_arguments_0(
                handler.as_ref().unchecked_ref(),
                180,
            );
            handler.forget();
        });
    }
    let selected_hash = selected_commit
        .read()
        .as_ref()
        .map(|commit| commit.full_hash.clone());
    let unavailable = unavailable_commit_hashes.read().clone();
    let active_selected_commit = selected_hash
        .as_ref()
        .and_then(|hash| {
            filtered_commits
                .iter()
                .find(|commit| &commit.full_hash == hash)
                .cloned()
        })
        .filter(|commit| !unavailable.iter().any(|hash| hash == &commit.full_hash))
        .or_else(|| {
            filtered_commits
                .iter()
                .find(|commit| !unavailable.iter().any(|hash| hash == &commit.full_hash))
                .cloned()
        });
    
    rsx! {
        // JSX: <div className="fl-tray-backdrop" onClick={onClose}/>
        div {
            class: "fl-tray-backdrop",
            onclick: move |_| on_close.call(())
        }
        
        // JSX: <aside className="fl-tray" role="dialog" aria-label={...}>
        aside {
            class: "fl-tray",
            role: "dialog",
            "aria-label": "{flake.name} commits",
            tabindex: "0",
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    evt.prevent_default();
                    on_close.call(());
                }
            },
            
            // Tray header - JSX lines 118-134
            header { class: "fl-tray-head",
                div { style: "display: flex; align-items: center; gap: 10px; min-width: 0; flex: 1;",
                    // Git icon
                    svg {
                        width: "18",
                        height: "18",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        style: "color: var(--cf-brand-purple); flex-shrink: 0;",
                        path { d: "M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4M9 18c-4.51 2-5-2-7-2" }
                    }
                    div { style: "min-width: 0;",
                        div { style: "display: flex; align-items: center; gap: 8px;",
                            span { style: "font-weight: 700; font-size: 15px;", "{flake.name}" }
                            span { class: "chip chip-unknown", style: "font-size: 10px;", "{flake.branch}" }
                            FlakeSyncChipNew { status: flake.status.clone(), error_msg: flake.error_msg.clone() }
                        }
                        div {
                            class: "mono",
                            style: "font-size: 11px; color: var(--cf-text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                            "{flake.url}"
                        }
                    }
                }
                div { style: "display: flex; gap: 6px; align-items: center;",
                    // Admin-only Sync and Edit buttons
                    if is_admin {
                        button { 
                            class: "btn btn-ghost focus-ring xs",
                            onclick: move |_| on_sync.call(flake.id),
                            // Inline sync icon (11px)
                            svg {
                                width: "11",
                                height: "11",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                style: "display: inline-block; vertical-align: middle; margin-right: 6px;",
                                path { d: "M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2" }
                            }
                            " Sync"
                        }
                        button {
                            class: "btn btn-ghost focus-ring xs",
                            onclick: move |_| on_edit.call(flake.id),
                            svg {
                                width: "11",
                                height: "11",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                style: "display: inline-block; vertical-align: middle; margin-right: 6px;",
                                circle { cx: "12", cy: "12", r: "3" }
                                path { d: "M19.4 15a1.7 1.7 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.82-.33 1.7 1.7 0 0 0-1 1.52V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1-1.52 1.7 1.7 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .33-1.82 1.7 1.7 0 0 0-1.52-1H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.52-1 1.7 1.7 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.82.33h.09a1.7 1.7 0 0 0 1-1.52V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.52 1.7 1.7 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.33 1.82v.09a1.7 1.7 0 0 0 1.52 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.52 1z" }
                            }
                            " Edit"
                        }
                    }
                    button {
                        class: "btn-icon focus-ring",
                        onclick: move |_| on_close.call(()),
                        "aria-label": "Close",
                        // Inline X icon (16px)
                        svg {
                            width: "16",
                            height: "16",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M18 6 6 18M6 6l12 12" }
                        }
                    }
                }
            }

            if let Some(msg) = notice {
                div {
                    style: "margin: 0 12px 10px; padding: 8px 10px; border: 1px solid var(--cf-divider); border-radius: 8px; font-size: 12px; color: var(--cf-text-secondary); background: color-mix(in oklab, var(--cf-page-bg) 35%, var(--cf-card-bg));",
                    "{msg}"
                }
            }
            
            // Body: Two-pane layout - JSX lines 136-192 (commit list)
            div { class: "fl-tray-body",
                // Left pane: Commit list with timeline
                nav {
                    class: "fl-tray-commits",
                    id: "{commits_scroll_id}",
                    onscroll: move |_| {
                        if !has_more_commits {
                            return;
                        }
                        let Some(window) = window() else {
                            return;
                        };
                        let Some(document) = window.document() else {
                            return;
                        };
                        let Some(element) = document.get_element_by_id(&commits_scroll_id) else {
                            return;
                        };
                        let scroll_top = element.scroll_top();
                        let client_height = element.client_height();
                        let scroll_height = element.scroll_height();
                        if scroll_top + client_height + 96 >= scroll_height {
                            let next = *visible_limit.read() + LOAD_MORE_STEP;
                            visible_limit.set(next.min(filtered_commits.len()));
                        }
                    },
                    // Search bar - JSX lines 140-150
                    div { class: "fl-tray-commits-search",
                        // Search icon
                        svg {
                            width: "12",
                            height: "12",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            style: "color: var(--cf-text-muted); flex-shrink: 0;",
                            circle { cx: "11", cy: "11", r: "8" }
                            path { d: "m21 21-4.3-4.3" }
                        }
                        input {
                            class: "input focus-ring",
                            placeholder: "Filter commits…",
                            value: "{commit_query}",
                            oninput: move |evt| {
                                commit_query.set(evt.value().clone());
                                visible_limit.set(INITIAL_VISIBLE_COMMITS);
                            },
                            style: "background: transparent; border: none; padding: 4px 0; font-size: 12px; flex: 1;"
                        }
                        span { 
                            style: "font-size: 10px; color: var(--cf-text-muted);",
                            "{filtered_commits.len()}/{commits.len()}"
                        }
                    }
                    
                    // Commit items grouped by time bucket - JSX lines 151-185
                    if commits_loading {
                        div { class: "empty", style: "margin: 12px;", "Loading commits…" }
                    } else if let Some(err) = commits_error {
                        div { class: "empty", style: "margin: 12px;", "Unable to load commits: {err}" }
                    } else {
                        CommitsListNew {
                            commits: visible_commits,
                            selected_commit: active_selected_commit.clone(),
                            on_select: move |commit| selected_commit.set(Some(commit))
                        }
                        if has_more_commits {
                            div {
                                class: "empty",
                                style: "margin: 12px; font-size: 11px;",
                                "Scroll to load more commits…"
                            }
                        }
                    }
                }
                
                // Right pane: Commit detail - JSX lines 192-260
                section { class: "fl-tray-detail",
                    if let Some(commit) = active_selected_commit.clone() {
                        {
                            let selected_commit_hash_for_unavailable = commit.full_hash.clone();
                            let filtered_commits_for_unavailable = filtered_commits.clone();
                            let mut unavailable_commit_hashes = unavailable_commit_hashes.clone();
                            rsx! {
                                CommitDetailNew {
                                    key: "{commit.full_hash}",
                                    flake: flake.clone(),
                                    flake_id: flake.id,
                                    commit,
                                    on_request_timeline_refresh: on_sync,
                                    on_commit_unavailable: move |missing_hash: String| {
                                        if missing_hash != selected_commit_hash_for_unavailable {
                                            return;
                                        }

                                        unavailable_commit_hashes.with_mut(|hashes: &mut Vec<String>| {
                                            if !hashes.iter().any(|hash| hash == &missing_hash) {
                                                hashes.push(missing_hash.clone());
                                            }
                                        });

                                        let replacement = filtered_commits_for_unavailable
                                            .iter()
                                            .find(|candidate| {
                                                candidate.full_hash != missing_hash
                                                    && !unavailable_commit_hashes
                                                        .read()
                                                        .iter()
                                                        .any(|hash| hash == &candidate.full_hash)
                                            })
                                            .cloned();
                                        selected_commit.set(replacement);
                                    },
                                    on_history_rewrite_conflict: on_history_rewrite_conflict,
                                }
                            }
                        }
                    } else {
                        div { class: "empty", style: "margin: 32px;",
                            "No commits found for this flake."
                        }
                    }
                }
            }
        }

    }
}


// ============================================================================
// CommitsList - Time-bucketed commits with timeline rail
// Matching JSX lines 151-185
// ============================================================================

#[allow(dead_code)]
#[component]
fn CommitsListNew(
    commits: Vec<MockCommitItem>,
    selected_commit: Option<MockCommitItem>,
    on_select: EventHandler<MockCommitItem>
) -> Element {
    // Group commits by time bucket (Today, This week, Earlier)
    let mut today = Vec::new();
    let mut this_week = Vec::new();
    let mut earlier = Vec::new();
    
    let now = Utc::now();
    for commit in &commits {
        let age = now.signed_duration_since(commit.committed_at);
        if age < Duration::days(1) {
            today.push(commit.clone());
        } else if age < Duration::days(7) {
            this_week.push(commit.clone());
        } else {
            earlier.push(commit.clone());
        }
    }
    
    rsx! {
        // Today bucket
        if !today.is_empty() {
            CommitBucketNew {
                bucket_name: "Today",
                commits: today.clone(),
                selected_commit: selected_commit.clone(),
                on_select,
                is_last_bucket: this_week.is_empty() && earlier.is_empty()
            }
        }
        
        // This week bucket
        if !this_week.is_empty() {
            CommitBucketNew {
                bucket_name: "This week",
                commits: this_week.clone(),
                selected_commit: selected_commit.clone(),
                on_select,
                is_last_bucket: earlier.is_empty()
            }
        }
        
        // Earlier bucket
        if !earlier.is_empty() {
            CommitBucketNew {
                bucket_name: "Earlier",
                commits: earlier.clone(),
                selected_commit: selected_commit.clone(),
                on_select,
                is_last_bucket: true
            }
        }
        
        // Empty state
        if commits.is_empty() {
            div { class: "empty", style: "margin: 24px;",
                "No commits match."
            }
        }
    }
}

// ============================================================================
// CommitBucket - Single time bucket with commits
// ============================================================================

#[allow(dead_code)]
#[component]
fn CommitBucketNew(
    bucket_name: &'static str,
    commits: Vec<MockCommitItem>,
    selected_commit: Option<MockCommitItem>,
    on_select: EventHandler<MockCommitItem>,
    is_last_bucket: bool
) -> Element {
    let total_commits = commits.len();
    
    rsx! {
        div {
            // Bucket header - JSX line 153
            div { class: "fl-commits-bucket", "{bucket_name}" }
            
            // Commit items - JSX lines 154-183
            for (i, commit) in commits.iter().enumerate() {
                {
                    let is_selected = selected_commit.as_ref().map_or(false, |sel| sel.sha == commit.sha);
                    let is_last_in_bucket = i == total_commits - 1;
                    let pipeline_status = MockPipelineStatus {
                        eval: commit.eval_status.clone(),
                        build: commit.build_status.clone(),
                    };
                    
                    rsx! {
                        CommitItemNew {
                            commit: commit.clone(),
                            is_selected,
                            is_last: is_last_in_bucket && is_last_bucket,
                            pipeline_status,
                            on_select
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// CommitItem - Single commit with timeline rail and pipeline dots
// Matching JSX lines 161-182
// ============================================================================

#[allow(dead_code)]
#[component]
fn CommitItemNew(
    commit: MockCommitItem,
    is_selected: bool,
    is_last: bool,
    pipeline_status: MockPipelineStatus,
    on_select: EventHandler<MockCommitItem>
) -> Element {
    let item_class = if is_selected {
        "fl-commit-item active"
    } else {
        "fl-commit-item"
    };
    
    let sha_color = if is_selected {
        "var(--cf-brand-purple)"
    } else {
        "var(--cf-text-primary)"
    };
    
    let dot_class = if is_selected {
        "fl-dot sel"
    } else {
        "fl-dot"
    };
    
    rsx! {
        div {
            class: "{item_class}",
            onclick: move |_| on_select.call(commit.clone()),
            
            // Timeline rail - JSX lines 165-168
            div { class: "fl-rail",
                div { class: "{dot_class}" }
                if !is_last {
                    div { class: "fl-stem" }
                }
            }
            
            // Commit content - JSX lines 169-180
            div { style: "min-width: 0; flex: 1;",
                // SHA and timestamp - JSX lines 170-173
                div { style: "display: flex; align-items: baseline; gap: 6px;",
                    span { 
                        class: "mono",
                        style: "font-size: 11px; font-weight: 700; color: {sha_color};",
                        "{commit.sha}"
                    }
                    span { 
                        style: "font-size: 11px; color: var(--cf-text-muted); margin-left: auto;",
                        "{commit.at}"
                    }
                }
                
                // Commit message - JSX line 174
                div {
                    class: "truncate",
                    style: "font-size: 12px; margin-top: 3px; color: var(--cf-text-primary);",
                    "{commit.msg}"
                }
                
                // Pipeline status and author - JSX lines 175-179
                div { style: "display: flex; gap: 5px; margin-top: 6px; flex-wrap: wrap;",
                    if let Some(eval_status) = &pipeline_status.eval {
                        PipelineDotNew { kind: "eval", val: eval_status.clone() }
                    }
                    if let Some(build_status) = &pipeline_status.build {
                        PipelineDotNew { kind: "build", val: build_status.clone() }
                    }
                    span { 
                        class: "mono",
                        style: "font-size: 10px; color: var(--cf-text-muted); margin-left: auto;",
                        "{commit.author}"
                    }
                }
            }
        }
    }
}

// ============================================================================
// PipelineDot - Small colored square with E/B label
// Matching JSX lines 396-414
// ============================================================================

#[allow(dead_code)]
#[component]
fn PipelineDotNew(kind: &'static str, val: String) -> Element {
    let color = match val.as_str() {
        "complete" | "cache-pushed" | "up-to-date" => "#34d399",
        "building" | "pending" | "in_progress" => "#60a5fa",
        "failed" => "#f87171",
        "behind" => "#f59e0b",
        _ => "#6b7280",
    };
    
    let label = match kind {
        "eval" => "E",
        "build" => "B",
        _ => &kind[0..1].to_uppercase(),
    };
    
    let title = format!("{}: {}", kind, val);
    let background = format!("color-mix(in oklab, {} 15%, transparent)", color);
    
    rsx! {
        span {
            title: "{title}",
            style: "
                display: inline-flex;
                align-items: center;
                justify-content: center;
                width: 14px;
                height: 14px;
                border-radius: 4px;
                font-size: 9px;
                font-weight: 700;
                color: {color};
                background: {background};
                font-family: var(--font-mono);
            ",
            "{label}"
        }
    }
}

// ============================================================================
// CommitDetail - Right pane showing commit metadata and file changes
// Matching JSX lines 193-259
// ============================================================================

#[allow(dead_code)]
#[component]
fn CommitDetailNew(
    flake: MockFlakeItem,
    flake_id: i32,
    commit: MockCommitItem,
    on_request_timeline_refresh: EventHandler<i32>,
    on_commit_unavailable: EventHandler<String>,
    on_history_rewrite_conflict: EventHandler<(i32, String)>,
) -> Element {
    let mut selected_file_label = use_signal(String::new);
    let mut active_modal_file = use_signal(|| None::<MockFileItem>);
    let mut rewrite_prompted = use_signal(|| false);
    let mut auto_refresh_requested = use_signal(|| false);
    let mut unavailable_commit_handled = use_signal(|| false);
    let diff_resource = use_resource({
        let commit_hash = commit.full_hash.clone();
        move || {
            let request_hash = commit_hash.clone();
            async move {
                fetch_commit_diff(flake_id, &request_hash)
                    .await
                    .map(|r| r.diff)
                    .map_err(|err| err)
            }
        }
    });

    let files_loading = matches!(diff_resource.read().as_ref(), None);
    let files_error = match diff_resource.read().as_ref() {
        Some(Err(err)) => Some(err.clone()),
        _ => None,
    };
    let files = match diff_resource.read().as_ref() {
        Some(Ok(diff)) if !diff.trim().is_empty() => map_diff_to_file_cards(diff),
        _ => Vec::new(),
    };
    let selected_file_name = if files.iter().any(|f| f.name == *selected_file_label.read()) {
        Some(selected_file_label.read().clone())
    } else {
        None
    };
    let total_additions: i32 = files.iter().map(|f| f.add).sum();
    let total_deletions: i32 = files.iter().map(|f| f.del).sum();
    let total_files_changed = files.len() as i32;

    if let Some(error) = files_error.as_ref() {
        if is_commit_not_found_diff_error(error) && !*auto_refresh_requested.read() {
            auto_refresh_requested.set(true);
            on_request_timeline_refresh.call(flake_id);
        }

        if is_commit_not_found_diff_error(error) && !*unavailable_commit_handled.read() {
            unavailable_commit_handled.set(true);
            on_commit_unavailable.call(commit.full_hash.clone());
        }

        if !*rewrite_prompted.read() {
            if let Some((conflict_flake_id, detail)) =
                extract_history_rewrite_conflict(error, Some(flake_id))
            {
                rewrite_prompted.set(true);
                on_history_rewrite_conflict.call((conflict_flake_id, detail));
            }
        }
    }

    let pipeline = MockPipelineStatus {
        eval: commit.eval_status.clone(),
        build: commit.build_status.clone(),
    };
    
    rsx! {
        // Commit header - JSX lines 196-217
        div { class: "fl-tray-commit-h",
            // SHA and message - JSX lines 197-200
            div { style: "display: flex; align-items: baseline; gap: 10px; flex-wrap: wrap;",
                span { 
                    class: "mono",
                    style: "font-size: 14px; font-weight: 700; color: var(--cf-brand-purple);",
                    "{commit.sha}"
                }
                span { 
                    style: "font-size: 14px; font-weight: 600;",
                    "{commit.msg}"
                }
            }
            
            // Metadata row - JSX lines 201-207
            div { style: "display: flex; gap: 12px; margin-top: 6px; font-size: 11px; color: var(--cf-text-muted); flex-wrap: wrap;",
                span {
                    // User icon (inline)
                    svg {
                        width: "11",
                        height: "11",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        style: "display: inline-block; vertical-align: middle; margin-right: 4px;",
                        path { d: "M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" }
                        circle { cx: "12", cy: "7", r: "4" }
                    }
                    span { class: "mono", "{commit.author}" }
                }
                span { "{commit.at}" }
                span { style: "color: #34d399;", "+{total_additions}" }
                span { style: "color: #f87171;", "-{total_deletions}" }
                span { "{total_files_changed} files" }
            }
            
            // Pipeline strip - JSX lines 209-216
            div { class: "fl-pipeline",
                PipelinePillNew { stage: "eval", val: pipeline.eval.clone() }
                PipelineArrowNew {}
                PipelinePillNew { stage: "build", val: pipeline.build.clone() }
                PipelineArrowNew {}
                RolloutPillNew { on: commit.rollout_on, total: commit.rollout_total.max(commit.rollout_on), failed: 0 }
            }
        }
        
        // Files changed section - JSX lines 219-255
        div { class: "fl-files-section",
            // Section header - JSX lines 221-227
            div { class: "fl-tray-section-h",
                span { "{files.len()} files changed · select a file to view diff" }
                span { style: "color: var(--cf-text-muted); font-weight: 400; font-size: 10px;",
                    span { style: "color: #34d399;", "+{total_additions}" }
                    " / "
                    span { style: "color: #f87171;", "-{total_deletions}" }
                }
            }
            
            // Files grid - JSX lines 228-254
            div { class: "fl-files-grid",
                if files_loading {
                    div { class: "empty", "Loading file changes…" }
                } else if let Some(err) = files_error {
                    if is_commit_not_found_diff_error(&err) {
                        div {
                            class: "empty",
                            "This commit is no longer available after a history rewrite."
                            div {
                                style: "margin-top: 6px; font-size: 11px; color: var(--cf-text-muted);",
                                "Refresh flakes and select a newer commit."
                            }
                        }
                    } else {
                        div {
                            class: "empty",
                            "Unable to load file changes: {err}"
                        }
                    }
                } else if files.is_empty() {
                    div { class: "empty", "No file changes in this commit." }
                } else {
                    for file in files {
                        {
                            let mut selected_file_label = selected_file_label.clone();
                            let mut active_modal_file = active_modal_file.clone();
                            rsx! {
                                FileCardNew {
                                    file: file.clone(),
                                    is_selected: selected_file_name.as_ref().is_some_and(|name| name == &file.name),
                                    on_select: move |picked: MockFileItem| {
                                        selected_file_label.set(picked.name.clone());
                                        active_modal_file.set(Some(picked));
                                    },
                                }
                            }
                        }
                    }
                }
            }

            if let Some(file) = active_modal_file.read().clone() {
                DiffModalNew {
                    file,
                    commit: commit.clone(),
                    flake: flake.clone(),
                    on_close: move |_| active_modal_file.set(None),
                }
            }
        }
    }
}

// ============================================================================
// PipelinePill - Larger chip for eval/build status
// Matching JSX lines 417-424
// ============================================================================

#[allow(dead_code)]
#[component]
fn PipelinePillNew(stage: &'static str, val: Option<String>) -> Element {
    let Some(val_str) = val else {
        return rsx! { span { class: "chip chip-unknown", style: "font-weight: 600;", "N/A" } };
    };
    
    let (chip_class, label) = match (stage, val_str.as_str()) {
        ("eval", "complete") => ("chip-healthy", "Eval ✓"),
        ("eval", "pending") => ("chip-info", "Eval…"),
        ("eval", "failed") => ("chip-critical", "Eval ✗"),
        ("build", "cache-pushed") => ("chip-healthy", "Cached"),
        ("build", "complete") => ("chip-healthy", "Built"),
        ("build", "building") => ("chip-info", "Building"),
        ("build", "failed") => ("chip-critical", "Build ✗"),
        ("build", "pending") => ("chip-unknown", "Queued"),
        _ => ("chip-unknown", val_str.as_str()),
    };
    
    rsx! {
        span { class: "chip {chip_class}", style: "font-weight: 600;", "{label}" }
    }
}

// ============================================================================
// PipelineArrow - Simple arrow separator
// Matching JSX line 427
// ============================================================================

#[allow(dead_code)]
#[component]
fn PipelineArrowNew() -> Element {
    rsx! {
        span { style: "color: var(--cf-text-muted); font-size: 11px;", "→" }
    }
}

// ============================================================================
// RolloutPill - Rollout status with progress bar
// Matching JSX lines 431-442
// ============================================================================

#[allow(dead_code)]
#[component]
fn RolloutPillNew(on: i32, total: i32, failed: i32) -> Element {
    let pct = if total > 0 { (on as f32) / (total as f32) } else { 0.0 };
    let chip_class = if failed > 0 {
        "chip-critical"
    } else if pct == 1.0 {
        "chip-healthy"
    } else if pct == 0.0 {
        "chip-unknown"
    } else {
        "chip-warning"
    };
    
    rsx! {
        span { 
            class: "chip {chip_class}",
            style: "display: inline-flex; align-items: center; gap: 6px; font-weight: 600;",
            // Server icon
            svg {
                width: "10",
                height: "10",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                rect { x: "2", y: "2", width: "20", height: "8", rx: "2", ry: "2" }
                rect { x: "2", y: "14", width: "20", height: "8", rx: "2", ry: "2" }
                line { x1: "6", y1: "6", x2: "6.01", y2: "6" }
                line { x1: "6", y1: "18", x2: "6.01", y2: "18" }
            }
            "Rollout {on}/{total}"
            div { style: "width: 32px; height: 3px; background: rgba(255,255,255,0.2); border-radius: 99px; overflow: hidden;",
                div { style: "width: {pct * 100.0}%; height: 100%; background: currentColor;" }
            }
        }
    }
}

// ============================================================================
// FileCard - Single file change card with add/del stats
// Matching JSX lines 232-252
// ============================================================================

#[allow(dead_code)]
#[component]
fn FileCardNew(file: MockFileItem, is_selected: bool, on_select: EventHandler<MockFileItem>) -> Element {
    let file_for_click = file.clone();
    let total = (file.add + file.del) as f32 + 0.001;
    let add_pct = ((file.add as f32 / total) * 100.0).round() as i32;
    let del_pct = ((file.del as f32 / total) * 100.0).round() as i32;
    
    // Split path into filename and directory
    let parts: Vec<&str> = file.name.split('/').collect();
    let filename = parts.last().unwrap_or(&"");
    let directory = if parts.len() > 1 {
        parts[..parts.len()-1].join("/")
    } else {
        ".".to_string()
    };
    
    let card_class = if is_selected {
        "fl-file-card focus-ring active"
    } else {
        "fl-file-card focus-ring"
    };

    rsx! {
        button {
            class: "{card_class}",
            onclick: move |_| on_select.call(file_for_click.clone()),
            
            // File header - JSX lines 236-242
            div { class: "fl-file-card-head",
                // File icon
                svg {
                    width: "13",
                    height: "13",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    style: "opacity: 0.55; flex-shrink: 0;",
                    path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                    polyline { points: "14 2 14 8 20 8" }
                }
                div { style: "min-width: 0; flex: 1;",
                    div { 
                        class: "fl-file-name truncate",
                        title: "{file.name}",
                        "{filename}"
                    }
                    div { 
                        class: "fl-file-path truncate",
                        title: "{file.name}",
                        "{directory}"
                    }
                }
            }
            
            // File stats - JSX lines 243-250
            div { class: "fl-file-stats",
                span { class: "mono", style: "font-size: 11px; color: #34d399;", "+{file.add}" }
                span { class: "mono", style: "font-size: 11px; color: #f87171;", "-{file.del}" }
                div { class: "fl-file-bar",
                    div { style: "width: {add_pct}%; height: 100%; background: #34d399; display: inline-block; vertical-align: top;" }
                    div { style: "width: {del_pct}%; height: 100%; background: #f87171; display: inline-block; vertical-align: top;" }
                }
            }
        }
    }
}

#[allow(dead_code)]
#[component]
fn InlineFileDiffNew(file: MockFileItem, full_diff_text: String) -> Element {
    let diff_text = extract_diff_block_for_file_label(&full_diff_text, &file.name)
        .unwrap_or(full_diff_text.clone());
    if diff_text.trim().is_empty() {
        return rsx! {
            div { class: "empty", style: "margin-top: 12px;", "No diff content available for selected file." }
        };
    }

    let parsed_files = parse_unified_diff(&diff_text);
    let parsed_file = parsed_files.first().cloned();

    rsx! {
        div { style: "margin-top: 12px; border: 1px solid var(--cf-border); border-radius: 10px; overflow: hidden; background: var(--cf-surface-2);",
            div { style: "padding: 8px 10px; border-bottom: 1px solid var(--cf-border); display: flex; align-items: center; justify-content: space-between; gap: 8px;",
                span { class: "mono", style: "font-size: 11px; color: var(--cf-text-muted);", "Diff · {file.name}" }
                span { style: "font-size: 10px; color: #34d399;", "+{file.add}" }
                span { style: "font-size: 10px; color: #f87171;", "-{file.del}" }
            }

            if let Some(parsed) = parsed_file {
                div { style: "max-height: 320px; overflow: auto;",
                    for line in parsed.lines {
                        div {
                            class: "grid {line.class_name}",
                            style: "grid-template-columns: 3.2rem 3.2rem 1.4rem minmax(0, 1fr);",
                            div { class: "px-2 py-0.5 text-[10px] text-gray-500 text-right border-r border-gray-800", "{line.old_number.map(|n| n.to_string()).unwrap_or_default()}" }
                            div { class: "px-2 py-0.5 text-[10px] text-gray-500 text-right border-r border-gray-800", "{line.new_number.map(|n| n.to_string()).unwrap_or_default()}" }
                            div { class: "px-1 py-0.5 text-[11px] text-gray-400 border-r border-gray-800", "{line.prefix}" }
                            div {
                                class: if line.is_hunk_header {
                                    "px-2 py-0.5 text-[11px] font-mono text-sky-300"
                                } else {
                                    "px-2 py-0.5 text-[11px] font-mono text-gray-200 hljs language-{parsed.language}"
                                },
                                if line.is_hunk_header {
                                    "{line.content}"
                                } else {
                                    span { dangerous_inner_html: "{highlight_diff_fragment(&parsed.language, &line.content)}" }
                                }
                            }
                        }
                    }
                }
            } else {
                pre { class: "mono", style: "font-size: 11px; padding: 10px; overflow: auto;", "{diff_text}" }
            }
        }
    }
}

// ============================================================================
// DiffModal - Full-screen diff viewer with hunk navigation
// Matching JSX lines 270-393
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
struct DiffLine {
    line_type: String,  // "hunk", "meta", "add", "del", "ctx"
    text: String,
    old_no: Option<i32>,
    new_no: Option<i32>,
    hunk_idx: i32,
}

#[allow(dead_code)]
#[component]
fn DiffModalNew(
    file: MockFileItem,
    commit: MockCommitItem,
    flake: MockFlakeItem,
    on_close: EventHandler<()>
) -> Element {
    let diff_resource = use_resource({
        let flake_id = flake.id;
        let commit_hash = commit.full_hash.clone();
        move || {
            let request_hash = commit_hash.clone();
            async move {
                fetch_commit_diff(flake_id, &request_hash)
                    .await
                    .map(|r| r.diff)
                    .map_err(|err| err.to_string())
            }
        }
    });

    let diff_loading = matches!(diff_resource.read().as_ref(), None);
    let diff_error = match diff_resource.read().as_ref() {
        Some(Err(err)) => Some(err.clone()),
        _ => None,
    };

    let full_diff_text = match diff_resource.read().as_ref() {
        Some(Ok(diff)) => diff.clone(),
        _ => String::new(),
    };

    let diff_text = extract_diff_block_for_file_label(&full_diff_text, &file.name)
        .unwrap_or_else(|| full_diff_text.clone());
    let lines: Vec<&str> = diff_text.split('\n').collect();
    
    // Parse diff into annotated lines
    let mut annotated = Vec::new();
    let mut old_no = 0;
    let mut new_no = 0;
    let mut hunk_idx = -1;
    
    for line in lines {
        if line.starts_with("@@") {
            // Hunk header
            hunk_idx += 1;
            annotated.push(DiffLine {
                line_type: "hunk".to_string(),
                text: line.to_string(),
                old_no: None,
                new_no: None,
                hunk_idx,
            });
        } else if line.starts_with("+++") || line.starts_with("---") {
            // Metadata line
            annotated.push(DiffLine {
                line_type: "meta".to_string(),
                text: line.to_string(),
                old_no: None,
                new_no: None,
                hunk_idx,
            });
        } else if line.starts_with("+") {
            // Addition
            new_no += 1;
            annotated.push(DiffLine {
                line_type: "add".to_string(),
                text: line.to_string(),
                old_no: None,
                new_no: Some(new_no),
                hunk_idx,
            });
        } else if line.starts_with("-") {
            // Deletion
            old_no += 1;
            annotated.push(DiffLine {
                line_type: "del".to_string(),
                text: line.to_string(),
                old_no: Some(old_no),
                new_no: None,
                hunk_idx,
            });
        } else {
            // Context line
            old_no += 1;
            new_no += 1;
            annotated.push(DiffLine {
                line_type: "ctx".to_string(),
                text: line.to_string(),
                old_no: Some(old_no),
                new_no: Some(new_no),
                hunk_idx,
            });
        }
    }
    
    let hunks: Vec<_> = annotated.iter().filter(|r| r.line_type == "hunk").collect();
    let total_add = annotated.iter().filter(|r| r.line_type == "add").count();
    let total_del = annotated.iter().filter(|r| r.line_type == "del").count();
    let total_lines = annotated.iter().filter(|r| r.line_type != "meta").count();
    
    let mut wrap = use_signal(|| false);
    
    rsx! {
        // Backdrop - JSX line 331
        div {
            class: "modal-backdrop",
            onclick: move |_| on_close.call(()),
            style: "z-index: 90;",
            tabindex: "0",
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    evt.prevent_default();
                    on_close.call(());
                }
            },
            
            // Modal content - JSX line 332
            div {
                class: "diff-modal",
                onclick: move |evt| evt.stop_propagation(),
                
                // Header - JSX lines 333-368
                header { class: "diff-modal-head",
                    div { style: "min-width: 0; flex: 1;",
                        // Breadcrumb - JSX lines 335-340
                        div { style: "display: flex; align-items: center; gap: 8px; font-size: 11px; color: var(--cf-text-muted);",
                            // Git icon
                            svg {
                                width: "11",
                                height: "11",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4M9 18c-4.51 2-5-2-7-2" }
                            }
                            span { class: "mono", "{flake.name}" }
                            span { "·" }
                            span { class: "mono", "{commit.sha}" }
                            span { 
                                style: "overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                "{commit.msg}"
                            }
                        }
                        
                        // File info - JSX lines 342-348
                        div { style: "display: flex; align-items: center; gap: 10px; margin-top: 4px; flex-wrap: wrap;",
                            // File icon
                            svg {
                                width: "13",
                                height: "13",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                style: "opacity: 0.6;",
                                path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                                polyline { points: "14 2 14 8 20 8" }
                            }
                            span { class: "mono", style: "font-size: 13px; font-weight: 600;", "{file.name}" }
                            span { class: "chip chip-healthy", style: "font-size: 10px;", "+{total_add}" }
                            span { class: "chip chip-critical", style: "font-size: 10px;", "-{total_del}" }
                            span { 
                                style: "font-size: 11px; color: var(--cf-text-muted);",
                                "· {hunks.len()} hunk"
                                if hunks.len() != 1 { "s" }
                                " · {total_lines} lines"
                            }
                        }
                    }
                    
                    // Action buttons - JSX lines 350-367
                    div { style: "display: flex; gap: 6px; align-items: center;",
                        // Wrap toggle button - JSX lines 362-364
                        button {
                            class: if *wrap.read() { "btn-icon focus-ring active" } else { "btn-icon focus-ring" },
                            title: if *wrap.read() { "Disable line wrap" } else { "Wrap long lines" },
                            onclick: move |_| {
                                let current = *wrap.read();
                                wrap.set(!current);
                            },
                            // Rows icon
                            svg {
                                width: "14",
                                height: "14",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                rect { x: "3", y: "3", width: "18", height: "18", rx: "2", ry: "2" }
                                line { x1: "3", y1: "9", x2: "21", y2: "9" }
                                line { x1: "3", y1: "15", x2: "21", y2: "15" }
                            }
                        }
                        // Copy path button - JSX line 365
                        // Constructs a URL to the file on the git remote
                        button {
                            class: "btn-icon focus-ring",
                            title: "Copy path",
                            onclick: {
                                let file_path = file.name.clone();
                                let commit_hash = commit.full_hash.clone();
                                let repo_url = flake.url.clone();
                                move |_| {
                                    // Construct the URL to the file on the git remote
                                    let url_to_copy = construct_git_file_url(&repo_url, &commit_hash, &file_path);
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        use wasm_bindgen::JsCast;
                                        if let Some(win) = window() {
                                            let win_ref: &JsValue = win.as_ref();
                                            if let Ok(navigator) = Reflect::get(win_ref, &JsValue::from_str("navigator")) {
                                                if let Ok(clipboard) = Reflect::get(&navigator, &JsValue::from_str("clipboard")) {
                                                    if let Ok(write_text) = Reflect::get(&clipboard, &JsValue::from_str("writeText")) {
                                                        if let Ok(function) = write_text.dyn_into::<Function>() {
                                                            let _ = function.call1(&clipboard, &JsValue::from_str(&url_to_copy));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            // Link icon
                            svg {
                                width: "14",
                                height: "14",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" }
                                path { d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" }
                            }
                        }
                        // Close button - JSX line 366
                        button {
                            class: "btn-icon focus-ring",
                            title: "Close (Esc)",
                            onclick: move |_| on_close.call(()),
                            // X icon
                            svg {
                                width: "16",
                                height: "16",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M18 6 6 18M6 6l12 12" }
                            }
                        }
                    }
                }
                
                // Diff body - JSX lines 369-389
                div { class: "diff-modal-body",
                    if diff_loading {
                        div { class: "empty", style: "padding: 16px;", "Loading diff…" }
                    } else if let Some(err) = diff_error {
                        div { class: "empty", style: "padding: 16px;", "Unable to load diff: {err}" }
                    } else if diff_text.trim().is_empty() {
                        div { class: "empty", style: "padding: 16px;", "No diff available for this file." }
                    } else {
                        table {
                            class: if *wrap.read() { "diff-table wrap" } else { "diff-table" },
                            tbody {
                                for (i, row) in annotated.iter().enumerate() {
                                    {
                                        if row.line_type == "meta" {
                                            rsx! { "" }
                                        } else if row.line_type == "hunk" {
                                            rsx! {
                                                tr { 
                                                    key: "{i}",
                                                    class: "diff-hunk",
                                                    td { colspan: 3, "{row.text}" }
                                                }
                                            }
                                        } else {
                                            let row_class = format!("diff-row diff-{}", row.line_type);
                                            rsx! {
                                                tr { 
                                                    key: "{i}",
                                                    class: "{row_class}",
                                                    td { class: "diff-gutter mono", "{row.old_no.map(|n| n.to_string()).unwrap_or_default()}" }
                                                    td { class: "diff-gutter mono", "{row.new_no.map(|n| n.to_string()).unwrap_or_default()}" }
                                                    td { class: "diff-code mono", "{row.text}" }
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
        }
    }
}
