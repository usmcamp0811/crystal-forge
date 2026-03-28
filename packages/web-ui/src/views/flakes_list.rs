//! Flakes list view with table/card toggle.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
#[cfg(target_arch = "wasm32")]
use js_sys::Object;
use uuid::Uuid;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use web_sys::console;
use web_sys::{window, Node};

use crate::api::client::{
    ApiClientError, accept_flake_history_rewrite, create_flake, delete_flake, fetch_commit_diff,
    fetch_flake_timelines, fetch_flake_timelines_for_ids, fetch_flakes, request_sync_all_flakes,
    request_sync_flake,
    update_flake,
};
use crate::api::models::{
    BuildStatus as ApiBuildStatus, CreateFlakeRequest, FlakeRegistryItem, FlakeTimeline,
    UpdateFlakeRequest,
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

fn preview_systems(systems: &[String]) -> &[String] {
    let end = systems.len().min(60);
    &systems[..end]
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
}

#[derive(Clone, Debug, PartialEq)]
struct EditFlakeDraft {
    id: i32,
    name: String,
    repo_url: String,
    branch: String,
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
    let mut flakes = use_signal(mock_flakes);
    let loading_flakes = use_signal(|| true);
    let server_notice = use_signal(|| None::<String>);
    let mut flake_timelines = use_signal(Vec::<FlakeTimeline>::new);
    let mut show_add_form = use_signal(|| false);
    let mut add_error = use_signal(|| None::<String>);
    let mut draft = use_signal(|| NewFlakeDraft {
        name: String::new(),
        repo_url: String::new(),
        branch: String::new(),
    });
    let mut pending_remove = use_signal(|| None::<FlakeListItem>);
    let mut editing_flake = use_signal(|| None::<EditFlakeDraft>);
    let mut edit_error = use_signal(|| None::<String>);
    let mut refreshing_flake = use_signal(|| None::<i32>);
    let mut selected_history_flake = use_signal(|| None::<i32>);
    let mut selected_history_commit = use_signal(|| None::<String>);
    let mut sync_note = use_signal(|| None::<String>);
    let mut last_manual_sync = use_signal(|| None::<DateTime<Utc>>);
    let mut rewrite_prompt = use_signal(|| None::<(i32, String, String)>);
    let mut timeline_generation = use_signal(|| 0_u64);

    let current_flakes = flakes.read().clone();
    let environments = unique_environments(&current_flakes);

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
                        server_notice.set(Some(format!(
                            "Flake API unavailable, using local sample data: {error}"
                        )));
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
        let selected_history_flake = selected_history_flake.clone();
        let mut timeline_generation = timeline_generation.clone();
        
        // Memoize flake IDs to prevent effect from re-running on every render
        let flake_ids_memo = use_memo(move || {
            flakes.read().iter().map(|flake| flake.id).collect::<Vec<i32>>()
        });
        
        use_effect(move || {
            let flake_ids: Vec<i32> = flake_ids_memo.read().clone();
            let selected_flake_id = *selected_history_flake.read();
            let generation = *timeline_generation.read() + 1;
            timeline_generation.set(generation);
            spawn(async move {
                if flake_ids.is_empty() {
                    if *timeline_generation.read() == generation {
                        flake_timelines.set(Vec::new());
                    }
                    return;
                }

                let prioritized_ids = if let Some(selected_id) = selected_flake_id {
                    if flake_ids.iter().any(|id| *id == selected_id) {
                        let mut ordered = Vec::with_capacity(flake_ids.len());
                        ordered.push(selected_id);
                        ordered.extend(flake_ids.iter().copied().filter(|id| *id != selected_id));
                        ordered
                    } else {
                        flake_ids.clone()
                    }
                } else {
                    flake_ids.clone()
                };

                let initial_ids: Vec<i32> = prioritized_ids
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
                            if *timeline_generation.read() != generation {
                                return;
                            }
                            flake_timelines.set(merged_timelines.clone());
                        }
                        Err(_error) => {
                            // Fallback to full fetch if subset request fails for any reason.
                            match fetch_flake_timelines().await {
                                Ok(timelines) => {
                                    if *timeline_generation.read() == generation {
                                        flake_timelines.set(timelines);
                                    }
                                }
                                Err(_) => {
                                    if *timeline_generation.read() == generation {
                                        flake_timelines.set(Vec::new());
                                    }
                                }
                            }
                            return;
                        }
                    }
                }

                let remaining_ids: Vec<i32> = prioritized_ids
                    .iter()
                    .skip(INITIAL_TIMELINE_FLAKES)
                    .copied()
                    .collect();

                // Process remaining batches without triggering re-renders per batch
                let mut batch_count = 0;
                let total_batches = (remaining_ids.len() + TIMELINE_BATCH_SIZE - 1) / TIMELINE_BATCH_SIZE;
                
                for chunk in remaining_ids.chunks(TIMELINE_BATCH_SIZE) {
                    batch_count += 1;
                    match fetch_flake_timelines_for_ids(chunk).await {
                        Ok(timelines) => {
                            merged_timelines = merge_flake_timeline_batches(
                                merged_timelines,
                                timelines,
                                &flake_ids,
                            );
                            if *timeline_generation.read() != generation {
                                return;
                            }
                            // Only update signal after the final batch to reduce render churn
                            if batch_count == total_batches {
                                flake_timelines.set(merged_timelines.clone());
                            }
                        }
                        Err(_) => {
                            // Keep already-loaded timelines if a later batch fails.
                            // Still update on final batch even if this one failed
                            if batch_count == total_batches && *timeline_generation.read() == generation {
                                flake_timelines.set(merged_timelines.clone());
                            }
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
                            let mut last_manual_sync = last_manual_sync.clone();
                            let mut sync_note = sync_note.clone();
                            let mut rewrite_prompt = rewrite_prompt.clone();
                            let mut timeline_generation = timeline_generation.clone();
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

                                        let generation = *timeline_generation.read() + 1;
                                        timeline_generation.set(generation);

                                        match fetch_flake_timelines().await {
                                            Ok(timelines) => {
                                                if *timeline_generation.read() == generation {
                                                    timelines_signal.set(timelines);
                                                }
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
                            };

                            match create_flake(&request).await {
                                Ok(created) => {
                                    let mut values = flakes.read().clone();
                                    values.push(FlakeListItem::from_registry(created));
                                    values
                                        .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                                    flakes.set(values);
                                    draft.set(NewFlakeDraft {
                                        name: String::new(),
                                        repo_url: String::new(),
                                        branch: String::new(),
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
            }

            if let Some(editing) = editing_flake.read().clone() {
                EditFlakeDialog {
                    draft: editing,
                    error: edit_error,
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
                            };

                            match update_flake(next.id, &request).await {
                                Ok(updated) => {
                                    let mut values = flakes.read().clone();
                                    if let Some(target) = values.iter_mut().find(|item| item.id == updated.id)
                                    {
                                        target.name = updated.name;
                                        target.repo_url = updated.repo_url;
                                        target.branch = updated.branch;
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
                        let mut timeline_generation = timeline_generation.clone();
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

                                    let generation = *timeline_generation.read() + 1;
                                    timeline_generation.set(generation);
                                    if let Ok(timelines) = fetch_flake_timelines().await {
                                        if *timeline_generation.read() == generation {
                                            timelines_signal.set(timelines);
                                        }
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
                                                    class: "text-xs px-2 py-1 rounded transition-colors cf-action-link",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_edit.call(flake.id)
                                                    },
                                                    "Edit"
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
                class: "px-6 py-4 border-b border-gray-800 flex items-center justify-between",
                style: "background: linear-gradient(135deg, rgba(130, 105, 155, 0.42) 0%, rgba(17, 24, 39, 0.92) 100%);",
                div {
                    h3 { class: "text-lg font-semibold text-white", "{flake.name}" }
                    p { class: "text-xs text-gray-300 mt-1 font-mono", "{flake.repo_url}" }
                    p { class: "text-[11px] text-sky-300 mt-1 font-mono", "branch: {flake.branch}" }
                }
            }
            div {
                class: "px-6 py-3 bg-gray-800/50",
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
                class: "px-6 py-3 bg-gray-900 space-y-2",
                p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500", "Environments" }
                div {
                    class: "flex flex-wrap gap-2",
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
                class: "px-6 py-3 bg-gray-800/50 flex items-center justify-between",
                div {
                    class: "space-y-1",
                    p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500", "Latest Commit" }
                    p { class: "text-sm text-gray-200 font-mono", "{latest_commit}" }
                }
                if flake.system_count > 0 {
                    div {
                        class: "inline-flex items-center gap-2",
                        button {
                            class: "text-xs px-2 py-1 rounded transition-colors cf-action-link",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                on_edit.call(flake.id)
                            },
                            "Edit"
                        }
                        span {
                            class: "text-xs text-gray-500",
                            "In Use"
                        }
                    }
                } else {
                    div {
                        class: "inline-flex items-center gap-2",
                        button {
                            class: "text-xs px-2 py-1 rounded transition-colors cf-action-link",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                on_edit.call(flake.id)
                            },
                            "Edit"
                        }
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
) -> Element {
    use crate::hooks::websocket::{use_websocket_eval_stream, SystemEvalStatus};
    let navigator = use_navigator();

    let fallback_flake_id = flakes.first().map(|flake| flake.id).unwrap_or(0);
    let active_flake_id = (*selected_flake_id.read()).unwrap_or(fallback_flake_id);

    // Cache for loaded commit diffs
    let loaded_diffs = use_signal(|| HashMap::<(i32, String), String>::new());
    // Track current active commit hash to force re-render when diff loads
    let current_commit_key = use_signal(|| (0i32, String::new()));

    // Memoize commit building to prevent recomputation on every render
    let commits = use_memo(move || {
        let timelines = timelines.clone();
        build_flake_commits(&timelines, active_flake_id)
    });
    
    // Clone commits once for this render to avoid repeated .read() calls
    let commits_vec = commits.read().clone();

    // Only stream eval updates after an explicit commit selection.
    // Auto-subscribing to the newest commit can flood the client on busy instances.
    let active_commit_for_ws = selected_commit_hash
        .read()
        .as_ref()
        .and_then(|hash| commits_vec.iter().find(|commit| &commit.hash == hash))
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
                                                span { class: "px-2 py-1 rounded bg-slate-700/70 text-slate-200", "{commit.systems.len()} configs" }
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
                                    if commit.systems.is_empty() {
                                        p { class: "text-sm text-gray-500", "No nixosConfigurations discovered for this commit." }
                                    } else {
                                        div {
                                            class: "flex flex-wrap gap-2",
                                            for hostname in preview_systems(&commit.systems).iter() {
                                                {
                                                    let status = system_status.read().get(hostname).cloned();
                                                    let chip_style = system_chip_style(status.as_ref());
                                                    let chip_class = if status == Some(SystemEvalStatus::Evaluating) {
                                                        "px-2 py-1 rounded border text-xs font-mono animate-pulse"
                                                    } else {
                                                        "px-2 py-1 rounded border text-xs font-mono"
                                                    };
                                                    rsx! {
                                                        span {
                                                            key: "{hostname}",
                                                            class: "{chip_class}",
                                                            style: "{chip_style}",
                                                            "{hostname}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        if commit.systems.len() > 60 {
                                            p {
                                                class: "text-xs text-amber-300",
                                                "Showing first 60 of {commit.systems.len()} configurations to keep the UI responsive."
                                            }
                                        }
                                        p {
                                            class: "text-xs text-slate-400",
                                            "[CF system] means this config name matches a Crystal Forge system deployed at this commit."
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
        Card {
            title: Some("Register Flake".to_string()),
            children: rsx! {
                div {
                    class: "space-y-4",
                    p {
                        class: "text-sm {theme::text::SECONDARY}",
                        "Schema context: {FLAKE_TABLE_SCHEMA_NOTE}."
                    }
                    div {
                        class: "grid grid-cols-1 md:grid-cols-3 gap-4",
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
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
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
    on_change: EventHandler<EditFlakeDraft>,
    on_submit: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let draft_for_name = draft.clone();
    let draft_for_repo = draft.clone();
    let draft_for_branch = draft.clone();

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
            onclick: move |_| on_cancel.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 cf-modal-panel-34",
                onclick: |evt| evt.stop_propagation(),
                h3 {
                    class: "text-lg font-semibold text-white mb-2",
                    "Edit Flake"
                }
                p {
                    class: "text-sm {theme::text::SECONDARY} mb-4",
                    "Update flake name and repository URL."
                }
                div {
                    class: "space-y-4",
                    label {
                        class: "space-y-2 block",
                        span { class: "text-xs uppercase tracking-wide text-gray-500", "Flake Name" }
                        input {
                            class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                            value: "{draft.name}",
                            oninput: move |evt| {
                                let mut next = draft_for_name.clone();
                                next.name = evt.value();
                                on_change.call(next);
                            }
                        }
                    }
                    label {
                        class: "space-y-2 block",
                        span { class: "text-xs uppercase tracking-wide text-gray-500", "Repository URL" }
                        input {
                            class: "w-full rounded-lg px-3 py-2 text-sm font-mono {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                            value: "{draft.repo_url}",
                            oninput: move |evt| {
                                let mut next = draft_for_repo.clone();
                                next.repo_url = evt.value();
                                on_change.call(next);
                            }
                        }
                    }
                    label {
                        class: "space-y-2 block",
                        span { class: "text-xs uppercase tracking-wide text-gray-500", "Branch (optional)" }
                        input {
                            class: "w-full rounded-lg px-3 py-2 text-sm font-mono {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                            value: "{draft.branch}",
                            placeholder: "main",
                            oninput: move |evt| {
                                let mut next = draft_for_branch.clone();
                                next.branch = evt.value();
                                on_change.call(next);
                            }
                        }
                    }
                    if let Some(message) = error.read().clone() {
                        p { class: "text-sm text-red-300", "{message}" }
                    }
                }
                div {
                    class: "flex gap-3 mt-6",
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm text-white {theme::interactive::PRIMARY_BTN}",
                        onclick: move |_| on_submit.call(()),
                        "Save Changes"
                    }
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
        }));
        edit_error.set(None);
    }
}

fn refresh_flake_by_id(flake_id: i32, mut refreshing_flake: Signal<Option<i32>>, mut sync_note: Signal<Option<String>>) {
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
    let Some(timeline) = timelines.iter().find(|timeline| timeline.flake_id == flake_id) else {
        return Vec::new();
    };

    timeline
        .commits
        .iter()
        .map(|commit| {
            let short_hash = commit.hash.chars().take(7).collect::<String>();
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
                systems: commit.systems.clone(),
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
        ApiBuildStatus::Failed => "failed",
        ApiBuildStatus::Complete => "complete",
        ApiBuildStatus::Idle => "idle",
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
        ApiBuildStatus::Failed => {
            "background-color: #472726; border-color: #ef4444; color: #fee2e2;"
        }
        ApiBuildStatus::Complete => {
            "background-color: #1f3a2f; border-color: #22c55e; color: #dcfce7;"
        }
        ApiBuildStatus::Idle => "background-color: #2b303b; border-color: #495264; color: #cbd5e1;",
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
        ApiClientError::Status { code, body }
            if *code == 409
                && (body.to_ascii_lowercase().contains("history rewrite")
                    || body
                        .to_ascii_lowercase()
                        .contains("history_rewrite_detected")) =>
        {
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
}
