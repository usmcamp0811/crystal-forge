//! Builders management view - pixel-perfect JSX port.

use dioxus::prelude::*;

use crate::api;
use crate::api::models::BuilderSummary;
use crate::components::builders::{AddBuilderModal, BuilderPanel, EditBuilderModal};
use crate::components::loading::LoadingSpinner;
use crate::components::{Icon, IconName};
use crate::state::app_state::AppState;
use crate::state::auth;

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

#[derive(Clone, Copy, PartialEq)]
enum ViewMode {
    Cards,
    Table,
}

/// Builders management page matching BuildersView.jsx structure.
#[component]
pub fn BuildersView() -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let can_manage_builders = auth::is_admin(&app_state.read().auth);
    let mut query = use_signal(String::new);
    let mut status_filter = use_signal(|| "all".to_string());
    let mut arch_filter = use_signal(|| "all".to_string());
    let mut view_mode = use_signal(|| ViewMode::Cards);
    let mut show_add_modal = use_signal(|| false);
    let mut edit_builder_id = use_signal(|| None::<uuid::Uuid>);
    let mut view_builder = use_signal(|| None::<BuilderSummary>);
    let mut refresh_trigger = use_signal(|| 0);
    let from_setup = use_signal(came_from_setup);

    let builders = use_resource(move || async move {
        let _ = refresh_trigger();
        api::client::fetch_builders().await
    });

    let mut on_builder_added = move || {
        show_add_modal.set(false);
        refresh_trigger.set(refresh_trigger() + 1);
    };

    let mut on_builder_updated = move || {
        edit_builder_id.set(None);
        refresh_trigger.set(refresh_trigger() + 1);
    };

    let mut on_open_builder = move |b: BuilderSummary| {
        view_builder.set(Some(b));
    };

    let mut on_edit_builder = move |id: uuid::Uuid| {
        edit_builder_id.set(Some(id));
    };

    let mut on_close_builder_panel = move || {
        view_builder.set(None);
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 16px;",

            // Setup coach guidance
            if from_setup() {
                div {
                    "data-testid": "setup-coach-builders-callout",
                    style: "background:rgba(30,58,138,0.22); border:1px solid rgba(96,165,250,0.55); border-radius:8px; padding:12px 16px;",
                    p { style: "color:#dbeafe; font-size:12px; font-weight:700; margin:0; letter-spacing:0.03em; text-transform:uppercase;", "Setup Tour - Step 3 of 6" }
                    p { style: "color:#dbeafe; font-size:14px; font-weight:600; margin:4px 0 0 0;", "Connect a builder" }
                    p { style: "color:#bfdbfe; font-size:13px; margin:4px 0 0 0;", "Use Add Builder to register a worker that evaluates and builds your flake changes." }
                }
            }

            // Page head
            div {
                class: "page-head",
                div {
                    h1 { class: "page-title", "Builders" }
                    p {
                        class: "page-subtitle",
                        {
                            let builder_data = builders.read();
                            match &*builder_data {
                                Some(Ok(builders_list)) => {
                                    let total = builders_list.len();
                                    let running = builders_list.iter()
                                        .filter(|b| b.enabled && b.status == crate::api::models::BuilderStatus::Active)
                                        .count();
                                    let slots_used: i32 = builders_list.iter()
                                        .filter(|b| b.enabled)
                                        .map(|b| b.active_jobs)
                                        .sum();
                                    let slots_total: i32 = builders_list.iter()
                                        .filter(|b| b.enabled)
                                        .map(|b| b.max_concurrent_jobs)
                                        .sum();
                                    format!("{} of {} running · {}/{} slots used · 24h build metrics unavailable",
                                        running, total, slots_used, slots_total)
                                },
                                _ => "Loading...".to_string()
                            }
                        }
                    }
                }
                if can_manage_builders {
                    button {
                        class: "btn btn-primary focus-ring",
                        "data-coach-target": "builder",
                        onclick: move |_| show_add_modal.set(true),
                        Icon { name: IconName::Plus, size: 14 }
                        " Register builder"
                    }
                }
            }

            // Stat strip
            {
                let builder_data = builders.read();
                match &*builder_data {
                    Some(Ok(builders_list)) => {
                        let total = builders_list.len();
                        let running = builders_list.iter()
                            .filter(|b| b.enabled && b.status == crate::api::models::BuilderStatus::Active)
                            .count();
                        let slots_used: i32 = builders_list.iter().filter(|b| b.enabled).map(|b| b.active_jobs).sum();
                        let slots_total: i32 = builders_list.iter().filter(|b| b.enabled).map(|b| b.max_concurrent_jobs).sum();
                        let slot_pct = if slots_total > 0 {
                            ((slots_used as f64 / slots_total as f64) * 100.0).round() as i32
                        } else {
                            0
                        };
                        rsx! {
                            div {
                                class: "stat-strip",
                                div {
                                    class: "stat",
                                    span {
                                        class: "stat-accent",
                                        style: "--stat-color: #a78bfa;"
                                    }
                                    div { class: "stat-label", "Total" }
                                    div {
                                        class: "stat-value",
                                        style: "color: #a78bfa;",
                                        "{total}"
                                    }
                                }
                                div {
                                    class: "stat",
                                    span {
                                        class: "stat-accent",
                                        style: "--stat-color: #34d399;"
                                    }
                                    div { class: "stat-label", "Running" }
                                    div {
                                        class: "stat-value",
                                        style: "color: #34d399;",
                                        "{running}"
                                    }
                                }
                                div {
                                    class: "stat",
                                    span {
                                        class: "stat-accent",
                                        style: if slot_pct > 85 {
                                            "--stat-color: #fbbf24;"
                                        } else {
                                            "--stat-color: #60a5fa;"
                                        }
                                    }
                                    div { class: "stat-label", "Slot use" }
                                    div {
                                        class: "stat-value",
                                        style: if slot_pct > 85 {
                                            "color: #fbbf24;"
                                        } else {
                                            "color: #60a5fa;"
                                        },
                                        "{slot_pct}%"
                                    }
                                }
                                div {
                                    class: "stat",
                                    span {
                                        class: "stat-accent",
                                        style: "--stat-color: #34d399;"
                                    }
                                    div { class: "stat-label", "Built 24h" }
                                    div {
                                        class: "stat-value",
                                        style: "color: #34d399;",
                                        "—"
                                    }
                                }
                                div {
                                    class: "stat",
                                    span {
                                        class: "stat-accent",
                                        style: "--stat-color: #34d399;"
                                    }
                                    div { class: "stat-label", "Failed 24h" }
                                    div {
                                        class: "stat-value",
                                        style: "color: #34d399;",
                                        "—"
                                    }
                                }
                            }
                        }
                    },
                    _ => rsx! { div {} }
                }
            }

            // Filter bar
            {
                let builder_data = builders.read();
                match &*builder_data {
                    Some(Ok(builders_list)) => {
                        // Extract unique architectures
                        let mut arches: Vec<String> = builders_list.iter()
                            .map(|b| b.arch.clone())
                            .collect::<std::collections::HashSet<_>>()
                            .into_iter()
                            .collect();
                        arches.sort();

                        // Apply filters
                        let filtered: Vec<_> = builders_list.iter()
                            .filter(|b| {
                                // Status filter - explicitly define each filter based on enabled and status
                                let status_match = match status_filter().as_str() {
                                    "all" => true,
                                    "running" => b.enabled && b.status == crate::api::models::BuilderStatus::Active,
                                    "paused" => !b.enabled || b.status == crate::api::models::BuilderStatus::Inactive,
                                    "offline" => b.enabled && b.status == crate::api::models::BuilderStatus::Offline,
                                    _ => false,
                                };

                                // Arch filter
                                let arch_match = arch_filter() == "all" || b.arch == arch_filter();

                                // Query filter
                                let query_match = if query().is_empty() {
                                    true
                                } else {
                                    let q = query().to_lowercase();
                                    b.name.to_lowercase().contains(&q) ||
                                    b.host.as_ref().map(|h| h.to_lowercase().contains(&q)).unwrap_or(false) ||
                                    b.arch.to_lowercase().contains(&q)
                                };

                                status_match && arch_match && query_match
                            })
                            .cloned()
                            .collect();

                        let filtered_count = filtered.len();

                        rsx! {
                            div {
                                class: "filterbar",

                                // Search input
                                div {
                                    class: "filter-search",
                                    style: "max-width: 320px;",
                                    Icon { name: IconName::Search }
                                    input {
                                        class: "input focus-ring",
                                        placeholder: "Search builders…",
                                        value: "{query}",
                                        oninput: move |e| query.set(e.value().clone())
                                    }
                                }

                                // Status filter segmented control
                                div {
                                    class: "seg",
                                    button {
                                        class: if status_filter() == "all" { "active" } else { "" },
                                        onclick: move |_| status_filter.set("all".to_string()),
                                        "all"
                                    }
                                    button {
                                        class: if status_filter() == "running" { "active" } else { "" },
                                        onclick: move |_| status_filter.set("running".to_string()),
                                        "running"
                                    }
                                    button {
                                        class: if status_filter() == "paused" { "active" } else { "" },
                                        onclick: move |_| status_filter.set("paused".to_string()),
                                        "paused"
                                    }
                                    button {
                                        class: if status_filter() == "offline" { "active" } else { "" },
                                        onclick: move |_| status_filter.set("offline".to_string()),
                                        "offline"
                                    }
                                }

                                // Architecture dropdown
                                select {
                                    class: "input filter-select focus-ring",
                                    style: "width: auto;",
                                    value: "{arch_filter}",
                                    onchange: move |e| arch_filter.set(e.value().clone()),
                                    option { value: "all", "All architectures" }
                                    for arch in arches {
                                        option { value: "{arch}", "{arch}" }
                                    }
                                }

                                // View mode toggle
                                div {
                                    class: "seg",
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

                                // Filter count
                                span {
                                    class: "filter-count",
                                    "{filtered_count} builders"
                                }
                            }

                            // Cards or table view
                            match view_mode() {
                                ViewMode::Cards => rsx! {
                                    div {
                                        class: "cards-grid",
                                        if filtered.is_empty() {
                                            div {
                                                class: "text-center py-12 border border-dashed border-slate-700 rounded-lg",
                                                p { class: "text-slate-400", "No builders match the current filters." }
                                            }
                                        } else {
                                            for builder in filtered {
                                                {
                                                    let b = builder.clone();
                                                    let id = builder.id;
                                                    rsx! {
                                                        BuilderCard {
                                                            key: "{id}",
                                                            builder: builder.clone(),
                                                            can_manage: can_manage_builders,
                                                            on_open: move |_| on_open_builder(b.clone()),
                                                            on_edit: move |_| on_edit_builder(id)
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                ViewMode::Table => rsx! {
                                    div {
                                        class: "card",
                                        style: "overflow: hidden;",
                                        if filtered.is_empty() {
                                            div {
                                                class: "text-center py-12",
                                                p { class: "text-slate-400", "No builders match the current filters." }
                                            }
                                        } else {
                                            table {
                                                class: "sys-table",
                                                thead {
                                                    tr {
                                                        th { "Builder" }
                                                        th { "Status" }
                                                        th { "Arch · envs" }
                                                        th { "Resources" }
                                                        th { "Slot use" }
                                                        th { "Built 24h" }
                                                        th { "Last seen" }
                                                        th { style: "text-align: right;", " " }
                                                    }
                                                }
                                                tbody {
                                                    for builder in filtered {
                                                        {
                                                            let b = builder.clone();
                                                            let id = builder.id;
                                                            rsx! {
                                                                BuilderRow {
                                                                    key: "{id}",
                                                                    builder: builder.clone(),
                                                                    can_manage: can_manage_builders,
                                                                    on_open: move |_| on_open_builder(b.clone()),
                                                                    on_edit: move |_| on_edit_builder(id)
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
                    },
                    Some(Err(e)) => rsx! {
                        div {
                            class: "border border-red-500/30 bg-red-500/10 rounded-lg p-4",
                            p { class: "text-red-400", "⚠️ Failed to load builders: {e}" }
                        }
                    },
                    None => rsx! {
                        LoadingSpinner {}
                    }
                }
            }
        }

        // Builder detail side panel
        {
            if let Some(builder) = view_builder() {
                let cloned = builder.clone();
                rsx! {
                    BuilderPanel {
                        key: "{builder.id}",
                        builder: builder.clone(),
                        on_close: move |_| on_close_builder_panel(),
                        on_edit: move |_| {
                            on_close_builder_panel();
                            on_edit_builder(cloned.id);
                        }
                    }
                }
            } else {
                rsx! {}
            }
        }

        // Modals
        if show_add_modal() && can_manage_builders {
            AddBuilderModal {
                on_close: move |_| show_add_modal.set(false),
                on_success: move |_| on_builder_added(),
                show_onboarding_callouts: from_setup()
            }
        }

        if can_manage_builders {
            if let Some(id) = edit_builder_id() {
                EditBuilderModal {
                    builder_id: id,
                    on_close: move |_| edit_builder_id.set(None),
                    on_success: move |_| on_builder_updated()
                }
            }
        }
    }
}

// BuilderCard and BuilderRow components will be defined in separate files
use crate::components::builders::{BuilderCard, BuilderRow};
