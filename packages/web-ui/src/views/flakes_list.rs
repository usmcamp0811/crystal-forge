//! Flakes list view with table/card toggle.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
#[cfg(target_arch = "wasm32")]
use js_sys::Object;
use uuid::Uuid;
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::Closure;
use web_sys::{Node, window};
#[cfg(target_arch = "wasm32")]
use web_sys::console;

use crate::api::client::{create_flake, delete_flake, fetch_commit_diff, fetch_flakes, fetch_flake_timelines};
use crate::api::models::{CreateFlakeRequest, FlakeRegistryItem, FlakeTimeline};
use crate::components::layout::Card;
use crate::theme;
use crate::views::systems_mock::mock_system_details;

const VIEW_PREF_KEY: &str = "crystal_forge.flakes.view";
const FLAKE_TABLE_SCHEMA_NOTE: &str = "flakes(name, repo_url UNIQUE)";

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
            latest_commit: None,
            system_count: item.system_count.max(0) as usize,
            environments: Vec::new(),
            last_synced_at: Utc::now(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct FlakeHistoryCommit {
    hash: String,
    message: String,
    author: String,
    committed_at: DateTime<Utc>,
    files_changed: usize,
    insertions: usize,
    deletions: usize,
    diff: String,
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
}

#[derive(Clone, Debug, PartialEq)]
struct EditFlakeDraft {
    id: i32,
    name: String,
    repo_url: String,
}

/// Flakes list with toggles and filters.
#[component]
pub fn FlakesListView() -> Element {
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
    });
    let mut pending_remove = use_signal(|| None::<FlakeListItem>);
    let mut editing_flake = use_signal(|| None::<EditFlakeDraft>);
    let mut edit_error = use_signal(|| None::<String>);
    let mut selected_history_flake = use_signal(|| None::<i32>);
    let mut selected_history_commit = use_signal(|| None::<String>);
    let mut sync_note = use_signal(|| None::<String>);
    let mut last_manual_sync = use_signal(|| None::<DateTime<Utc>>);

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
        use_effect(move || {
            spawn(async move {
                match fetch_flake_timelines().await {
                    Ok(timelines) => {
                        flake_timelines.set(timelines);
                    }
                    Err(_error) => {
                        // Fall back to mock timelines on error
                        flake_timelines.set(crate::views::dashboard::mock_flake_timelines());
                    }
                }
            });
        });
    }

    rsx! {
        div {
            class: "space-y-6",
            id: "{container_id}",
            header {
                class: "flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Flake Registry" }
                    p { class: "text-sm {theme::text::SECONDARY}", "Track flake repositories and deployment coverage." }
                }
                div {
                    class: "flex items-center gap-3",
                    button {
                        class: "px-3 py-2 rounded-lg text-sm font-medium border border-blue-500/50 text-blue-200 hover:text-white hover:bg-blue-500/20 transition-colors",
                        onclick: move |_| {
                            let mut next = flakes.read().clone();
                            let timelines = flake_timelines.read();
                            let changed = sync_flake_registry(&mut next, &timelines);
                            flakes.set(next);
                            let now = Utc::now();
                            last_manual_sync.set(Some(now));
                            sync_note.set(Some(format!(
                                "Polled {} flakes from source ({} updated).",
                                flakes.read().len(),
                                changed
                            )));
                        },
                        "Sync from Source"
                    }
                    button {
                        class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                        onclick: move |_| {
                            let next = !*show_add_form.read();
                            show_add_form.set(next);
                            add_error.set(None);
                        },
                        if *show_add_form.read() {
                            "Close"
                        } else {
                            "Add Flake"
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
                    class: "text-xs px-3 py-2 rounded-lg border text-blue-100",
                    style: "background-color: #23354B; border-color: #406084;",
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
                    class: "text-xs px-3 py-2 rounded-lg border text-amber-100",
                    style: "background-color: #493E26; border-color: #8C7041;",
                    "{message}"
                }
            }

            if *show_add_form.read() {
                AddFlakeForm {
                    draft: draft,
                    error: add_error,
                    on_cancel: move |_| {
                        draft.set(NewFlakeDraft {
                            name: String::new(),
                            repo_url: String::new(),
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

                        let mut values = flakes.read().clone();
                        if let Some(target) = values.iter_mut().find(|item| item.id == next.id) {
                            target.name = next.name.trim().to_string();
                            target.repo_url = next.repo_url.trim().to_string();
                        }
                        values.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                        flakes.set(values);
                        editing_flake.set(None);
                        edit_error.set(None);
                    }
                }
            }

            if let Some(flake) = pending_remove.read().clone() {
                RemoveFlakeDialog {
                    flake_name: flake.name.clone(),
                    system_count: flake.system_count,
                    on_cancel: move |_| pending_remove.set(None),
                    on_confirm: move |_| {
                        let mut flakes = flakes.clone();
                        let mut pending_remove = pending_remove.clone();
                        let mut server_notice = server_notice.clone();
                        let remove_id = flake.id;
                        spawn(async move {
                            match delete_flake(remove_id).await {
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
                                        class: if is_selected {
                                            "bg-blue-900/35 hover:bg-blue-900/45 transition cursor-pointer"
                                        } else {
                                            "hover:bg-gray-800/40 transition cursor-pointer"
                                        },
                                        onclick: move |_| on_select_history_flake.call(flake.id),
                                        td { class: "{theme::spacing::TABLE_CELL} text-sm text-white", "{flake.name}" }
                                        td { class: "{theme::spacing::TABLE_CELL} text-sm text-gray-300 font-mono", "{flake.repo_url}" }
                                        td { class: "{theme::spacing::TABLE_CELL} text-sm text-gray-200", "{flake.system_count}" }
                                        td { class: "{theme::spacing::TABLE_CELL} text-sm {theme::text::SECONDARY}", "{environments_label(&flake)}" }
                                        td { class: "{theme::spacing::TABLE_CELL} text-sm text-gray-300 font-mono", "{latest_commit_label(&flake)}" }
                                        td {
                                            class: "{theme::spacing::TABLE_CELL} text-right",
                                            div {
                                                class: "inline-flex items-center gap-2",
                                                button {
                                                    class: "text-xs px-2 py-1 rounded transition-colors",
                                                    style: "color: #D6C3E8;",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_edit.call(flake.id)
                                                    },
                                                    "Edit"
                                                }
                                                if flake.system_count > 0 {
                                                    span {
                                                        class: "text-xs text-gray-500",
                                                        "In Use"
                                                    }
                                                } else {
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
}

#[component]
fn FlakeCard(
    flake: FlakeListItem,
    selected_history_flake_id: Option<i32>,
    on_select_history_flake: EventHandler<i32>,
    on_remove: EventHandler<i32>,
    on_edit: EventHandler<i32>,
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
                }
            }
            div {
                class: "px-6 py-3 bg-gray-800/50",
                div {
                    class: "flex flex-wrap items-center gap-2 text-xs",
                    span {
                        class: "inline-flex px-2 py-1 rounded border text-gray-100",
                        style: "background-color: #2B303B; border-color: #495264;",
                        "{flake.system_count} systems"
                    }
                    span {
                        class: "inline-flex px-2 py-1 rounded border text-gray-100",
                        style: "background-color: #23363A; border-color: #3D6870;",
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
                                class: "inline-flex px-2 py-1 text-xs rounded border text-blue-100",
                                style: "background-color: #253449; border-color: #3E5B82;",
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
                            class: "text-xs px-2 py-1 rounded transition-colors",
                            style: "color: #D6C3E8;",
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
                            class: "text-xs px-2 py-1 rounded transition-colors",
                            style: "color: #D6C3E8;",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                on_edit.call(flake.id)
                            },
                            "Edit"
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
    let history = build_flake_history(&timelines);
    
    // Cache for loaded commit diffs
    let loaded_diffs = use_signal(|| HashMap::<(i32, String), String>::new());
    // Track current active commit hash to force re-render when diff loads
    let current_commit_key = use_signal(|| (0i32, String::new()));

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

    let active_flake_id = selected_flake_id
        .read()
        .to_owned()
        .unwrap_or_else(|| flakes[0].id);
    let active_flake = flakes
        .iter()
        .find(|flake| flake.id == active_flake_id)
        .cloned()
        .unwrap_or_else(|| flakes[0].clone());
    let commits = history.get(&active_flake.id).cloned().unwrap_or_default();

    let active_commit = selected_commit_hash
        .read()
        .as_ref()
        .and_then(|hash| commits.iter().find(|commit| &commit.hash == hash))
        .map(|commit| commit.clone())
        .or_else(|| commits.first().cloned());
    
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
                console::log_1(&format!("Effect running - hash: {}, loaded: {}", &commit_hash[..7.min(commit_hash.len())], already_loaded).into());
                
                if !already_loaded {
                    let commit_hash = commit_hash.clone();
                    let flake_id = flake_id;
                    let mut loaded_diffs_inner = loaded_diffs.clone();
                    let mut current_key_inner = current_key.clone();
                    
                    #[cfg(target_arch = "wasm32")]
                    console::log_1(&format!("Fetching diff for {}...", &commit_hash[..7.min(commit_hash.len())]).into());
                    
                    spawn(async move {
                        match fetch_commit_diff(flake_id, &commit_hash).await {
                            Ok(response) => {
                                #[cfg(target_arch = "wasm32")]
                                console::log_1(&format!("Diff loaded! {} bytes", response.diff.len()).into());
                                loaded_diffs_inner.write().insert(key.clone(), response.diff);
                                current_key_inner.set(key);
                            }
                            Err(e) => {
                                #[cfg(target_arch = "wasm32")]
                                console::log_1(&format!("Error: {}", e).into());
                                loaded_diffs_inner.write().insert(
                                    key.clone(),
                                    format!("Error loading diff: {}\n\nCommit: {}", e, commit_hash)
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
                            class: "rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden",
                            style: "background: linear-gradient(180deg, #131B29 0%, #0F141D 100%);",
                            div {
                                class: "px-3 py-2 border-b {theme::surface::CARD_BORDER} text-xs uppercase tracking-wide text-gray-400",
                                "Timeline"
                            }
                            div {
                                class: "max-h-[68vh] overflow-y-auto",
                                if commits.is_empty() {
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
                                            for commit in commits.iter() {
                                                {
                                                    let is_active = active_commit
                                                        .as_ref()
                                                        .map(|value| value.hash == commit.hash)
                                                        .unwrap_or(false);
                                                    let short_hash = commit.hash.chars().take(7).collect::<String>();
                                                    let commit_time = commit.committed_at.format("%b %d %H:%M").to_string();
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
                                                                p { class: "text-sm text-white font-semibold text-left truncate", style: "text-align: left;", "{commit.message}" }
                                                                div {
                                                                    class: "mt-2 flex flex-wrap items-center gap-2 text-[10px]",
                                                                    span {
                                                                        class: "inline-flex items-center px-2.5 py-1 rounded border font-mono leading-none",
                                                                        style: "background-color: rgba(130, 105, 155, 0.22); border-color: #82699B; color: #E5D8F3;",
                                                                        "{short_hash}"
                                                                    }
                                                                    span {
                                                                        class: "inline-flex items-center px-2.5 py-1 rounded border text-gray-100 leading-none",
                                                                        style: "background-color: #2B303B; border-color: #495264;",
                                                                        "{commit.author}"
                                                                    }
                                                                    span {
                                                                        class: "text-[10px] text-gray-400",
                                                                        "{commit_time}"
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
                                div {
                                    class: "px-4 py-3 border-b {theme::surface::CARD_BORDER} space-y-2",
                                    p { class: "text-base text-white font-semibold text-left", "{commit.message}" }
                                    div {
                                        class: "flex flex-wrap items-center gap-2 text-xs",
                                        span {
                                            class: "font-mono px-2.5 py-1 rounded border",
                                            style: "background-color: rgba(130, 105, 155, 0.22); border-color: #82699B; color: #E5D8F3;",
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
                                            "w-full text-left px-3 py-2 bg-blue-500/15"
                                        } else {
                                            "w-full text-left px-3 py-2 hover:bg-gray-800/70"
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
    on_submit: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
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
                        class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                        label {
                            class: "space-y-2",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Flake Name" }
                            input {
                                class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{draft.read().name}",
                                placeholder: "prod-core",
                                oninput: move |evt| {
                                    let mut next = draft.read().clone();
                                    next.name = evt.value();
                                    draft.set(next);
                                },
                            }
                        }
                        label {
                            class: "space-y-2",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Repository URL" }
                            input {
                                class: "w-full rounded-lg px-3 py-2 text-sm font-mono {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{draft.read().repo_url}",
                                placeholder: "https://github.com/org/repo",
                                oninput: move |evt| {
                                    let mut next = draft.read().clone();
                                    next.repo_url = evt.value();
                                    draft.set(next);
                                },
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
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let warning = if system_count == 0 {
        "This removes the flake from the registry. Related commits are deleted by cascade."
            .to_string()
    } else {
        format!("This flake is linked to {system_count} systems and cannot be removed.")
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4",
            style: "position: fixed; inset: 0; z-index: 60; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
            onclick: move |_| on_cancel.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6",
                style: "width: 100%; max-width: 30rem;",
                onclick: |evt| evt.stop_propagation(),
                h3 {
                    class: "text-lg font-semibold text-white mb-2",
                    "Remove flake {flake_name}?"
                }
                p {
                    class: "text-sm {theme::text::SECONDARY} mb-6",
                    "{warning}"
                }
                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    if system_count == 0 {
                        button {
                            class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-red-500 hover:bg-red-400 text-white",
                            onclick: move |_| on_confirm.call(()),
                            "Remove"
                        }
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

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4",
            style: "position: fixed; inset: 0; z-index: 60; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
            onclick: move |_| on_cancel.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6",
                style: "width: 100%; max-width: 34rem;",
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
        }));
        edit_error.set(None);
    }
}

fn validate_new_flake(draft: &NewFlakeDraft, existing: &[FlakeListItem]) -> Result<(), String> {
    let name = draft.name.trim();
    let repo_url = draft.repo_url.trim();

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

    Ok(())
}

fn validate_flake_edit(draft: &EditFlakeDraft, existing: &[FlakeListItem]) -> Result<(), String> {
    let name = draft.name.trim();
    let repo_url = draft.repo_url.trim();

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

fn build_flake_history(timelines: &[FlakeTimeline]) -> HashMap<i32, Vec<FlakeHistoryCommit>> {
    let mut history = HashMap::new();

    for timeline in timelines {
        let commits: Vec<FlakeHistoryCommit> = timeline
            .commits
            .iter()
            .map(|commit| {
                // Diff will be loaded on-demand when user views the commit
                FlakeHistoryCommit {
                    hash: commit.hash.clone(),
                    message: if commit.message.is_empty() {
                        // Provide a placeholder if message is empty
                        format!("Commit {}", &commit.hash[..7])
                    } else {
                        commit.message.clone()
                    },
                    author: if commit.author.is_empty() {
                        "Unknown".to_string()
                    } else {
                        commit.author.clone()
                    },
                    committed_at: commit.committed_at,
                    files_changed: 0, // Will be calculated from diff when loaded
                    insertions: 0,
                    deletions: 0,
                    diff: String::new(), // Empty initially, loaded on-demand
                }
            })
            .collect();

        history.insert(timeline.flake_id, commits);
    }

    history
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
