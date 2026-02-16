//! Flakes list view with table/card toggle.

use std::collections::HashMap;

use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use uuid::Uuid;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;
use web_sys::{window, Node};

use crate::components::layout::Card;
use crate::theme;
use crate::views::systems_list::mock_system_details;

const VIEW_PREF_KEY: &str = "crystal_forge.flakes.view";

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

#[derive(Clone, Debug)]
struct FlakeListItem {
    id: i32,
    name: String,
    repo_url: String,
    latest_commit: Option<String>,
    system_count: usize,
    environments: Vec<String>,
}

/// Flakes list with toggles and filters.
#[component]
pub fn FlakesListView() -> Element {
    let stored_view = LocalStorage::get::<String>(VIEW_PREF_KEY).ok();
    let mut view_mode = use_signal(|| FlakesViewMode::from_storage(stored_view));
    let query_view = prefers_view_from_query();
    let mut open_dropdown = use_signal(|| None::<FilterDropdown>);
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

    let flakes = mock_flakes();
    let environments = unique_environments(&flakes);

    let filtered_flakes: Vec<FlakeListItem> = flakes
        .into_iter()
        .filter(|flake| matches_environment(flake, &environment_filter.read()))
        .filter(|flake| matches_commit_state(flake, &commit_filter.read()))
        .filter(|flake| matches_size(flake, &size_filter.read()))
        .filter(|flake| matches_search(flake, &search.read()))
        .collect();

    rsx! {
        div {
            class: "space-y-6",
            id: "{container_id}",
            header {
                class: "flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Flakes" }
                    p { class: "text-sm {theme::text::SECONDARY}", "Track flake repositories and deployment coverage." }
                }
                ViewToggle {
                    view_mode: *view_mode.read(),
                    on_change: move |mode| {
                        view_mode.set(mode);
                        let _ = LocalStorage::set(VIEW_PREF_KEY, mode.as_storage());
                    }
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

            if filtered_flakes.is_empty() {
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
                        FlakeCard { flake }
                    }
                }
            } else {
                FlakesTable { flakes: filtered_flakes.clone() }
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
fn FlakesTable(flakes: Vec<FlakeListItem>) -> Element {
    let mut sort_column = use_signal(|| None::<SortColumn>);
    let mut sort_direction = use_signal(|| SortDirection::Asc);

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
        Card {
            title: Some("Flake Registry".to_string()),
            children: rsx! {
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
                            }
                        }
                        tbody {
                            class: "divide-y {theme::surface::DIVIDER}",
                            for flake in sorted_flakes {
                                tr {
                                    class: "hover:bg-gray-900/60 transition",
                                    td { class: "{theme::spacing::TABLE_CELL} text-sm text-white", "{flake.name}" }
                                    td { class: "{theme::spacing::TABLE_CELL} text-sm text-gray-300 font-mono", "{flake.repo_url}" }
                                    td { class: "{theme::spacing::TABLE_CELL} text-sm text-gray-200", "{flake.system_count}" }
                                    td { class: "{theme::spacing::TABLE_CELL} text-sm {theme::text::SECONDARY}", "{environments_label(&flake)}" }
                                    td { class: "{theme::spacing::TABLE_CELL} text-sm text-gray-300 font-mono", "{latest_commit_label(&flake)}" }
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
fn FlakeCard(flake: FlakeListItem) -> Element {
    let environments = environments_label(&flake);
    let latest_commit = latest_commit_label(&flake);

    rsx! {
        div {
            class: "rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden shadow-sm bg-gray-900",
            div {
                class: "px-6 py-4 border-b border-gray-800 flex items-center justify-between",
                div {
                    h3 { class: "text-lg font-semibold text-white", "{flake.name}" }
                    p { class: "text-xs text-gray-500 mt-1", "{flake.repo_url}" }
                }
                div {
                    class: "text-right",
                    span { class: "text-xs uppercase tracking-wide text-gray-500", "Systems" }
                    p { class: "text-2xl font-semibold text-white", "{flake.system_count}" }
                }
            }
            div {
                class: "px-6 py-4 bg-gray-800/40",
                p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500 mb-2", "Environments" }
                p { class: "text-sm text-gray-200", "{environments}" }
            }
            div {
                class: "px-6 py-4",
                p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500 mb-2", "Latest Commit" }
                p { class: "text-sm text-gray-200 font-mono", "{latest_commit}" }
            }
        }
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
            latest_commit: flake.latest_commit.clone(),
            system_count: 0,
            environments: Vec::new(),
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
