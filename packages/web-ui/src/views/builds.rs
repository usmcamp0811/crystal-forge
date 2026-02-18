//! Builds control center view.

use dioxus::prelude::*;

use crate::components::layout::Card;
use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkerStatus {
    Running,
    Paused,
    Draining,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuildStatus {
    Queued,
    Building,
    Stopping,
    Restarting,
    Failed,
    Complete,
    Canceled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DetailTab {
    Logs,
    Events,
    Artifacts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueueAction {
    StartAll,
    PauseAll,
    DrainAll,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkerAction {
    Start,
    Pause,
    Drain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuildAction {
    Stop,
    Restart,
    RunNext,
}

#[derive(Clone, Debug, PartialEq)]
struct WorkerItem {
    id: &'static str,
    name: &'static str,
    active_slots: usize,
    total_slots: usize,
    queue_depth: usize,
    status: WorkerStatus,
}

#[derive(Clone, Debug, PartialEq)]
struct BuildItem {
    id: i32,
    hostname: &'static str,
    flake: &'static str,
    commit: &'static str,
    branch: &'static str,
    worker_id: &'static str,
    queued_for: &'static str,
    runtime: Option<&'static str>,
    started_by: &'static str,
    status: BuildStatus,
    summary: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
struct BuildEvent {
    ts: &'static str,
    level: &'static str,
    message: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
struct BuildArtifact {
    name: &'static str,
    size: &'static str,
    hash: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
enum PendingAction {
    Queue(QueueAction),
    Worker {
        worker_id: &'static str,
        action: WorkerAction,
    },
    Build {
        build_id: i32,
        action: BuildAction,
    },
}

/// Builds control center page.
#[component]
pub fn BuildsView() -> Element {
    let mut workers = use_signal(mock_workers);
    let mut builds = use_signal(mock_builds);
    let mut selected_build = use_signal(|| Some(1_i32));
    let mut active_tab = use_signal(|| DetailTab::Logs);

    let follow_logs = use_signal(|| true);
    let pause_logs = use_signal(|| false);
    let wrap_logs = use_signal(|| false);
    let log_query = use_signal(String::new);

    let mut pending_action = use_signal(|| None::<PendingAction>);
    let mut last_action_note = use_signal(|| None::<String>);

    let queue_data = builds.read().clone();
    let worker_data = workers.read().clone();
    let selected = selected_build_data(selected_build.read().to_owned(), &queue_data);

    rsx! {
        div {
            class: "space-y-6",

            header {
                class: "flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Builds" }
                    p {
                        class: "text-sm {theme::text::SECONDARY}",
                        "Control active build workers, inspect queue state, and monitor live build logs."
                    }
                }
                div {
                    class: "flex flex-wrap items-center gap-2",
                    span {
                        class: "inline-flex items-center px-2 py-1 text-xs rounded border text-emerald-100",
                        style: "background-color: #1E3A2E; border-color: #2F6B4A;",
                        span { class: "w-2 h-2 rounded-full bg-emerald-300 mr-2 animate-pulse", }
                        "Live"
                    }
                    QueueActionButton {
                        label: "Start All",
                        onclick: move |_| pending_action.set(Some(PendingAction::Queue(QueueAction::StartAll))),
                    }
                    QueueActionButton {
                        label: "Pause All",
                        onclick: move |_| pending_action.set(Some(PendingAction::Queue(QueueAction::PauseAll))),
                    }
                    QueueActionButton {
                        label: "Drain All",
                        onclick: move |_| pending_action.set(Some(PendingAction::Queue(QueueAction::DrainAll))),
                    }
                }
            }

            MetricsRow {
                workers: worker_data.clone(),
                builds: queue_data.clone(),
            }

            WorkerStrip {
                workers: worker_data.clone(),
                on_action: move |(worker_id, action)| {
                    pending_action.set(Some(PendingAction::Worker { worker_id, action }))
                },
            }

            if let Some(note) = last_action_note.read().clone() {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-blue-100",
                    style: "background-color: #23354B; border-color: #406084;",
                    "{note}"
                }
            }

            div {
                class: "cf-builds-split",
                div {
                    BuildQueuePane {
                        builds: queue_data.clone(),
                        selected_id: selected_build,
                        on_build_action: move |(build_id, action)| {
                            pending_action.set(Some(PendingAction::Build { build_id, action }))
                        },
                    }
                }

                div {
                    BuildDetailPane {
                        selected: selected,
                        tab: active_tab,
                        on_tab_change: move |tab| active_tab.set(tab),
                        follow_logs: follow_logs,
                        pause_logs: pause_logs,
                        wrap_logs: wrap_logs,
                        log_query: log_query,
                    }
                }
            }

            if let Some(action) = pending_action.read().clone() {
                ConfirmActionModal {
                    action: action.clone(),
                    on_cancel: move |_| pending_action.set(None),
                    on_confirm: move |_| {
                        if let Some(next_action) = pending_action.read().clone() {
                            apply_action(next_action, &mut workers, &mut builds, &mut selected_build, &mut last_action_note);
                        }
                        pending_action.set(None);
                    }
                }
            }
        }
    }
}

#[component]
fn MetricsRow(workers: Vec<WorkerItem>, builds: Vec<BuildItem>) -> Element {
    let building = builds
        .iter()
        .filter(|b| matches!(b.status, BuildStatus::Building | BuildStatus::Restarting))
        .count();
    let queued = builds
        .iter()
        .filter(|b| matches!(b.status, BuildStatus::Queued))
        .count();
    let failed = builds
        .iter()
        .filter(|b| matches!(b.status, BuildStatus::Failed))
        .count();
    let active_workers = workers
        .iter()
        .filter(|w| w.status == WorkerStatus::Running)
        .count();

    rsx! {
        div {
            class: "grid grid-cols-2 md:grid-cols-4 gap-3",
            MetricBadge { label: "Building", value: building.to_string(), bg: "#23363A", border: "#3D6870" }
            MetricBadge { label: "Queued", value: queued.to_string(), bg: "#2E2E3F", border: "#4D4D72" }
            MetricBadge { label: "Failed", value: failed.to_string(), bg: "#44262A", border: "#7A3D48" }
            MetricBadge {
                label: "Workers",
                value: format!("{active_workers}/{}", workers.len()),
                bg: "#2B303B",
                border: "#495264",
            }
        }
    }
}

#[component]
fn MetricBadge(
    label: &'static str,
    value: String,
    bg: &'static str,
    border: &'static str,
) -> Element {
    rsx! {
        div {
            class: "rounded-lg border px-3 py-2",
            style: "background-color: {bg}; border-color: {border};",
            p { class: "text-[10px] uppercase tracking-wide text-gray-400", "{label}" }
            p { class: "text-sm text-white font-semibold", "{value}" }
        }
    }
}

#[component]
fn WorkerStrip(
    workers: Vec<WorkerItem>,
    on_action: EventHandler<(&'static str, WorkerAction)>,
) -> Element {
    rsx! {
        div {
            class: "grid grid-cols-1 lg:grid-cols-2 gap-3",
            for worker in workers {
                div {
                    key: "{worker.id}",
                    class: "rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden shadow-sm",
                    div {
                        class: "px-4 py-3 border-b border-gray-800 flex items-center justify-between",
                        style: "background: linear-gradient(135deg, rgba(130, 105, 155, 0.34) 0%, rgba(17, 24, 39, 0.92) 100%);",
                        div {
                            p { class: "text-sm text-white font-semibold", "{worker.name}" }
                            p { class: "text-xs {theme::text::SECONDARY}", "{worker.active_slots}/{worker.total_slots} active slots" }
                        }
                        span {
                            class: "inline-flex px-2 py-1 text-[10px] uppercase rounded border",
                            style: "{worker_status_style(worker.status)}",
                            "{worker.status_label()}"
                        }
                    }
                    div {
                        class: "px-4 py-3 bg-gray-900/80 flex items-center justify-between",
                        p { class: "text-xs text-gray-400", "Queue depth: {worker.queue_depth}" }
                        div {
                            class: "inline-flex items-center gap-2",
                            WorkerTextAction {
                                label: "Start",
                                on_click: move |_| on_action.call((worker.id, WorkerAction::Start)),
                            }
                            WorkerTextAction {
                                label: "Pause",
                                on_click: move |_| on_action.call((worker.id, WorkerAction::Pause)),
                            }
                            WorkerTextAction {
                                label: "Drain",
                                on_click: move |_| on_action.call((worker.id, WorkerAction::Drain)),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn WorkerTextAction(label: &'static str, on_click: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: "text-xs px-2 py-1 rounded transition-colors",
            style: "color: #D6C3E8;",
            onclick: move |evt| on_click.call(evt),
            "{label}"
        }
    }
}

#[component]
fn BuildQueuePane(
    builds: Vec<BuildItem>,
    selected_id: Signal<Option<i32>>,
    on_build_action: EventHandler<(i32, BuildAction)>,
) -> Element {
    let mut search = use_signal(String::new);

    let mut filtered: Vec<BuildItem> = builds
        .into_iter()
        .filter(|b| {
            let q = search.read().trim().to_lowercase();
            if q.is_empty() {
                true
            } else {
                b.hostname.to_lowercase().contains(&q)
                    || b.flake.to_lowercase().contains(&q)
                    || b.commit.to_lowercase().contains(&q)
            }
        })
        .collect();

    filtered.sort_by_key(|b| queue_sort_rank(b.status));

    rsx! {
        Card {
            title: Some("Queue".to_string()),
            children: rsx! {
                div {
                    class: "space-y-3",
                    input {
                        class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                        r#type: "search",
                        placeholder: "Search by host, flake, or commit...",
                        value: "{search.read()}",
                        oninput: move |evt| search.set(evt.value()),
                    }

                    div {
                        class: "space-y-2 max-h-[56vh] overflow-y-auto pr-1",
                        for build in filtered {
                            button {
                                key: "{build.id}",
                                class: "w-full rounded-xl border px-4 py-3 text-left transition",
                                style: "{queue_row_style(*selected_id.read() == Some(build.id), build.status)}",
                                onclick: move |_| selected_id.set(Some(build.id)),
                                div {
                                    class: "flex items-start justify-between gap-3",
                                    div {
                                        div {
                                            class: "flex items-center gap-2",
                                            p { class: "text-sm text-white font-semibold", "{build.hostname}" }
                                            span {
                                                class: "inline-flex px-2 py-0.5 text-[10px] rounded border text-blue-100",
                                                style: "background-color: #253449; border-color: #3E5B82;",
                                                "{build.flake}"
                                            }
                                        }
                                        p { class: "text-xs text-gray-300 mt-1", "{build.branch} · {short_commit(build.commit)}" }
                                    }
                                    div {
                                        class: "text-right",
                                        span {
                                            class: "inline-flex px-2 py-1 text-[10px] uppercase rounded border",
                                            style: "{build_status_badge_style(build.status)}",
                                            "{build.status_label()}"
                                        }
                                        p { class: "text-[10px] text-gray-400 mt-1", "{build.queued_for}" }
                                    }
                                }

                                div {
                                    class: "mt-2 rounded-md border border-gray-700/60 bg-gray-950/70 px-2 py-1",
                                    p { class: "text-[11px] text-gray-300 font-mono leading-5", "{build.summary}" }
                                }

                                div {
                                    class: "mt-3 flex flex-wrap items-center justify-between gap-2",
                                    div {
                                        class: "inline-flex items-center gap-2 text-[10px]",
                                        span {
                                            class: "inline-flex px-2 py-1 rounded border text-gray-100",
                                            style: "background-color: #2B303B; border-color: #495264;",
                                            "worker {build.worker_id}"
                                        }
                                        if let Some(runtime) = build.runtime {
                                            span {
                                                class: "inline-flex px-2 py-1 rounded border text-gray-100",
                                                style: "background-color: #23363A; border-color: #3D6870;",
                                                "runtime {runtime}"
                                            }
                                        }
                                    }
                                    div {
                                        class: "inline-flex items-center gap-2",
                                        if matches!(build.status, BuildStatus::Building | BuildStatus::Restarting) {
                                            button {
                                                class: "text-xs text-red-400 hover:text-red-300 px-2 py-1 rounded hover:bg-red-500/10 transition-colors",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    on_build_action.call((build.id, BuildAction::Stop));
                                                },
                                                "Stop"
                                            }
                                        }
                                        button {
                                            class: "text-xs px-2 py-1 rounded transition-colors",
                                            style: "color: #D6C3E8;",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                on_build_action.call((build.id, BuildAction::Restart));
                                            },
                                            "Restart"
                                        }
                                        if build.status == BuildStatus::Queued {
                                            button {
                                                class: "text-xs text-cyan-300 hover:text-cyan-200 px-2 py-1 rounded hover:bg-cyan-500/10 transition-colors",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    on_build_action.call((build.id, BuildAction::RunNext));
                                                },
                                                "Run Next"
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
fn BuildDetailPane(
    selected: Option<BuildItem>,
    tab: Signal<DetailTab>,
    on_tab_change: EventHandler<DetailTab>,
    follow_logs: Signal<bool>,
    pause_logs: Signal<bool>,
    wrap_logs: Signal<bool>,
    log_query: Signal<String>,
) -> Element {
    let Some(build) = selected else {
        return rsx! {
            Card {
                title: Some("Build Detail".to_string()),
                children: rsx! {
                    p { class: "text-sm {theme::text::SECONDARY}", "Select a queue item to inspect logs and build metadata." }
                }
            }
        };
    };

    let events = mock_events(build.id);
    let artifacts = mock_artifacts(build.id);
    let logs = filtered_logs(build.id, &log_query.read());

    rsx! {
        Card {
            title: Some("Build Detail".to_string()),
            children: rsx! {
                div {
                    class: "space-y-4",
                    div {
                        class: "rounded-lg border {theme::surface::CARD_BORDER} bg-gray-900/70 p-4",
                        div {
                            class: "flex flex-col md:flex-row md:items-center md:justify-between gap-3",
                            div {
                                p { class: "text-sm text-white font-semibold", "{build.hostname}" }
                                p { class: "text-xs text-gray-400", "{build.flake} · {build.branch} · {short_commit(build.commit)}" }
                                p { class: "text-xs text-gray-500 mt-1", "Queued by {build.started_by} · worker {build.worker_id}" }
                            }
                            span {
                                class: "inline-flex px-2 py-1 text-[10px] uppercase rounded border",
                                style: "{build_status_badge_style(build.status)}",
                                "{build.status_label()}"
                            }
                        }
                    }

                    div {
                        class: "inline-flex rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG}",
                        for item in [DetailTab::Logs, DetailTab::Events, DetailTab::Artifacts] {
                            button {
                                key: "{item.label()}",
                                class: "px-3 py-2 text-sm font-medium transition",
                                class: if *tab.read() == item {
                                    "bg-gray-700 text-white"
                                } else {
                                    "text-gray-400 hover:text-white"
                                },
                                onclick: move |_| on_tab_change.call(item),
                                "{item.label()}"
                            }
                        }
                    }

                    if *tab.read() == DetailTab::Logs {
                        div {
                            class: "space-y-3",
                            div {
                                class: "flex flex-wrap gap-2",
                                TogglePill { label: "Follow", value: follow_logs }
                                TogglePill { label: "Pause", value: pause_logs }
                                TogglePill { label: "Wrap", value: wrap_logs }
                                button {
                                    class: "text-xs text-gray-300 border border-gray-700 rounded px-2 py-1 hover:bg-gray-700",
                                    onclick: move |_| log_query.set(String::new()),
                                    "Clear"
                                }
                            }
                            input {
                                class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                r#type: "search",
                                placeholder: "Search logs...",
                                value: "{log_query.read()}",
                                oninput: move |evt| log_query.set(evt.value()),
                            }
                            pre {
                                class: "rounded-lg border border-gray-700 bg-gray-950 p-3 text-xs font-mono text-gray-200 overflow-auto",
                                style: if *wrap_logs.read() { "white-space: pre-wrap; max-height: 22rem;" } else { "white-space: pre; max-height: 22rem;" },
                                if logs.is_empty() {
                                    "No log lines match your filter."
                                } else {
                                    for line in logs {
                                        "{line}\n"
                                    }
                                }
                            }
                        }
                    }

                    if *tab.read() == DetailTab::Events {
                        div {
                            class: "space-y-2",
                            for event in events {
                                div {
                                    class: "rounded-lg border border-gray-700 bg-gray-900/60 p-3",
                                    div {
                                        class: "flex items-center justify-between gap-2",
                                        p { class: "text-xs text-gray-400", "{event.ts}" }
                                        span {
                                            class: "text-[10px] uppercase px-2 py-1 rounded border",
                                            style: "{event_level_style(event.level)}",
                                            "{event.level}"
                                        }
                                    }
                                    p { class: "text-sm text-gray-200 mt-1", "{event.message}" }
                                }
                            }
                        }
                    }

                    if *tab.read() == DetailTab::Artifacts {
                        div {
                            class: "space-y-2",
                            if artifacts.is_empty() {
                                p { class: "text-sm {theme::text::SECONDARY}", "No artifacts recorded yet for this build." }
                            } else {
                                for artifact in artifacts {
                                    div {
                                        class: "rounded-lg border border-gray-700 bg-gray-900/60 px-3 py-2 flex items-center justify-between gap-2",
                                        div {
                                            p { class: "text-sm text-white", "{artifact.name}" }
                                            p { class: "text-xs text-gray-500 font-mono", "{artifact.hash}" }
                                        }
                                        p { class: "text-xs text-gray-400", "{artifact.size}" }
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
fn TogglePill(label: &'static str, value: Signal<bool>) -> Element {
    rsx! {
        button {
            class: "text-xs rounded border px-2 py-1 transition",
            style: if *value.read() {
                "background-color: #253449; border-color: #3E5B82; color: #E2EBF7;"
            } else {
                "background-color: #212733; border-color: #394557; color: #9CA3AF;"
            },
            onclick: move |_| {
                let next = !*value.read();
                value.set(next);
            },
            "{label}"
        }
    }
}

#[component]
fn QueueActionButton(label: &'static str, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
            onclick: move |evt| onclick.call(evt),
            "{label}"
        }
    }
}

#[component]
fn ConfirmActionModal(
    action: PendingAction,
    on_cancel: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    let (title, description, confirm_label) = action_prompt(&action);

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4",
            style: "position: fixed; inset: 0; z-index: 60; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
            onclick: move |_| on_cancel.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6",
                style: "width: 100%; max-width: 30rem;",
                onclick: |evt| evt.stop_propagation(),
                h3 { class: "text-lg font-semibold text-white mb-2", "{title}" }
                p { class: "text-sm {theme::text::SECONDARY} mb-6", "{description}" }
                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm text-white {theme::interactive::PRIMARY_BTN}",
                        onclick: move |_| on_confirm.call(()),
                        "{confirm_label}"
                    }
                }
            }
        }
    }
}

fn action_prompt(action: &PendingAction) -> (&'static str, String, &'static str) {
    match action {
        PendingAction::Queue(QueueAction::StartAll) => (
            "Start all workers?",
            "This resumes queue processing on all build workers.".to_string(),
            "Start All",
        ),
        PendingAction::Queue(QueueAction::PauseAll) => (
            "Pause all workers?",
            "Queued builds will remain queued until workers are resumed.".to_string(),
            "Pause All",
        ),
        PendingAction::Queue(QueueAction::DrainAll) => (
            "Drain all workers?",
            "Workers finish active builds but stop taking new queued work.".to_string(),
            "Drain All",
        ),
        PendingAction::Worker {
            worker_id,
            action: WorkerAction::Start,
        } => (
            "Start worker?",
            format!("Worker {worker_id} will resume processing queued builds."),
            "Start",
        ),
        PendingAction::Worker {
            worker_id,
            action: WorkerAction::Pause,
        } => (
            "Pause worker?",
            format!("Worker {worker_id} will stop taking new work immediately."),
            "Pause",
        ),
        PendingAction::Worker {
            worker_id,
            action: WorkerAction::Drain,
        } => (
            "Drain worker?",
            format!("Worker {worker_id} will finish active builds, then idle."),
            "Drain",
        ),
        PendingAction::Build {
            build_id,
            action: BuildAction::Stop,
        } => (
            "Stop build?",
            format!(
                "Build #{build_id} will send stop to the active systemd unit and mark canceled."
            ),
            "Stop",
        ),
        PendingAction::Build {
            build_id,
            action: BuildAction::Restart,
        } => (
            "Restart build?",
            format!(
                "Build #{build_id} will cancel current systemd execution and rerun build commands."
            ),
            "Restart",
        ),
        PendingAction::Build {
            build_id,
            action: BuildAction::RunNext,
        } => (
            "Run this build next?",
            format!("Build #{build_id} will be promoted to the front of the queued set."),
            "Prioritize",
        ),
    }
}

fn apply_action(
    action: PendingAction,
    workers: &mut Signal<Vec<WorkerItem>>,
    builds: &mut Signal<Vec<BuildItem>>,
    selected_build: &mut Signal<Option<i32>>,
    note: &mut Signal<Option<String>>,
) {
    match action {
        PendingAction::Queue(queue_action) => {
            let mut next_workers = workers.read().clone();
            for worker in &mut next_workers {
                worker.status = match queue_action {
                    QueueAction::StartAll => WorkerStatus::Running,
                    QueueAction::PauseAll => WorkerStatus::Paused,
                    QueueAction::DrainAll => WorkerStatus::Draining,
                };
            }
            workers.set(next_workers);
            note.set(Some(format!("Applied {}", queue_action.label())));
        }
        PendingAction::Worker { worker_id, action } => {
            let mut next_workers = workers.read().clone();
            if let Some(worker) = next_workers.iter_mut().find(|w| w.id == worker_id) {
                worker.status = match action {
                    WorkerAction::Start => WorkerStatus::Running,
                    WorkerAction::Pause => WorkerStatus::Paused,
                    WorkerAction::Drain => WorkerStatus::Draining,
                };
            }
            workers.set(next_workers);
            note.set(Some(format!("Applied {} on {worker_id}", action.label())));
        }
        PendingAction::Build { build_id, action } => {
            let mut next_builds = builds.read().clone();
            match action {
                BuildAction::Stop => {
                    if let Some(target) = next_builds.iter_mut().find(|b| b.id == build_id) {
                        target.status = BuildStatus::Stopping;
                    }
                    if let Some(target) = next_builds.iter_mut().find(|b| b.id == build_id) {
                        target.status = BuildStatus::Canceled;
                    }
                    note.set(Some(format!("Stopped build #{build_id}")));
                }
                BuildAction::Restart => {
                    if let Some(target) = next_builds.iter_mut().find(|b| b.id == build_id) {
                        target.status = BuildStatus::Restarting;
                        target.runtime = Some("00:00");
                        target.queued_for = "restarting";
                    }
                    if let Some(target) = next_builds.iter_mut().find(|b| b.id == build_id) {
                        target.status = BuildStatus::Building;
                    }
                    note.set(Some(format!("Restarted build #{build_id}")));
                }
                BuildAction::RunNext => {
                    if let Some(index) = next_builds.iter().position(|b| b.id == build_id) {
                        let target = next_builds.remove(index);
                        let insert_idx = next_builds
                            .iter()
                            .position(|b| b.status == BuildStatus::Queued)
                            .unwrap_or(next_builds.len());
                        next_builds.insert(insert_idx, target);
                        selected_build.set(Some(build_id));
                        note.set(Some(format!("Prioritized build #{build_id}")));
                    }
                }
            }
            builds.set(next_builds);
        }
    }
}

fn selected_build_data(selected_id: Option<i32>, builds: &[BuildItem]) -> Option<BuildItem> {
    if let Some(id) = selected_id {
        builds.iter().find(|b| b.id == id).cloned()
    } else {
        builds.first().cloned()
    }
}

fn queue_sort_rank(status: BuildStatus) -> i32 {
    match status {
        BuildStatus::Building | BuildStatus::Restarting => 0,
        BuildStatus::Queued => 1,
        BuildStatus::Stopping => 2,
        BuildStatus::Failed => 3,
        BuildStatus::Complete => 4,
        BuildStatus::Canceled => 5,
    }
}

fn short_commit(commit: &str) -> String {
    commit.chars().take(7).collect()
}

fn queue_row_style(selected: bool, status: BuildStatus) -> String {
    let border = if selected { "#6D8FBA" } else { "#374151" };
    let bg = match status {
        BuildStatus::Building | BuildStatus::Restarting => "#1C2B3E",
        BuildStatus::Queued => "#242C3A",
        BuildStatus::Stopping => "#3C2F20",
        BuildStatus::Failed => "#3B232A",
        BuildStatus::Complete => "#1E362E",
        BuildStatus::Canceled => "#2C313A",
    };

    format!("background-color: {bg}; border-color: {border};")
}

fn build_status_badge_style(status: BuildStatus) -> &'static str {
    match status {
        BuildStatus::Queued => "background-color: #2E2E3F; border-color: #4D4D72; color: #D9D9FF;",
        BuildStatus::Building => {
            "background-color: #23363A; border-color: #3D6870; color: #D9F6F9;"
        }
        BuildStatus::Stopping => {
            "background-color: #4A3B22; border-color: #8C6A2F; color: #FDE8C6;"
        }
        BuildStatus::Restarting => {
            "background-color: #2E2A49; border-color: #675CAD; color: #E4DFFF;"
        }
        BuildStatus::Failed => "background-color: #44262A; border-color: #7A3D48; color: #FFDCE1;",
        BuildStatus::Complete => {
            "background-color: #1E3A2E; border-color: #2F6B4A; color: #D8FBE8;"
        }
        BuildStatus::Canceled => {
            "background-color: #2B303B; border-color: #495264; color: #E5E7EB;"
        }
    }
}

fn worker_status_style(status: WorkerStatus) -> &'static str {
    match status {
        WorkerStatus::Running => {
            "background-color: #1E3A2E; border-color: #2F6B4A; color: #D8FBE8;"
        }
        WorkerStatus::Paused => "background-color: #2B303B; border-color: #495264; color: #E5E7EB;",
        WorkerStatus::Draining => {
            "background-color: #4A3B22; border-color: #8C6A2F; color: #FDE8C6;"
        }
    }
}

fn event_level_style(level: &str) -> &'static str {
    match level {
        "error" => "background-color: #44262A; border-color: #7A3D48; color: #FFDCE1;",
        "warn" => "background-color: #4A3B22; border-color: #8C6A2F; color: #FDE8C6;",
        _ => "background-color: #2B303B; border-color: #495264; color: #E5E7EB;",
    }
}

fn filtered_logs(build_id: i32, query: &str) -> Vec<String> {
    let lines = mock_logs(build_id);
    if query.trim().is_empty() {
        return lines;
    }
    let q = query.to_lowercase();
    lines
        .into_iter()
        .filter(|line| line.to_lowercase().contains(&q))
        .collect()
}

impl WorkerStatus {
    fn label(self) -> &'static str {
        match self {
            WorkerStatus::Running => "running",
            WorkerStatus::Paused => "paused",
            WorkerStatus::Draining => "draining",
        }
    }
}

impl BuildStatus {
    fn label(self) -> &'static str {
        match self {
            BuildStatus::Queued => "queued",
            BuildStatus::Building => "building",
            BuildStatus::Stopping => "stopping",
            BuildStatus::Restarting => "restarting",
            BuildStatus::Failed => "failed",
            BuildStatus::Complete => "complete",
            BuildStatus::Canceled => "canceled",
        }
    }
}

impl QueueAction {
    fn label(self) -> &'static str {
        match self {
            QueueAction::StartAll => "start all",
            QueueAction::PauseAll => "pause all",
            QueueAction::DrainAll => "drain all",
        }
    }
}

impl WorkerAction {
    fn label(self) -> &'static str {
        match self {
            WorkerAction::Start => "start",
            WorkerAction::Pause => "pause",
            WorkerAction::Drain => "drain",
        }
    }
}

impl DetailTab {
    fn label(self) -> &'static str {
        match self {
            DetailTab::Logs => "Live Logs",
            DetailTab::Events => "Events",
            DetailTab::Artifacts => "Artifacts",
        }
    }
}

impl WorkerItem {
    fn status_label(&self) -> &'static str {
        self.status.label()
    }
}

impl BuildItem {
    fn status_label(&self) -> &'static str {
        self.status.label()
    }
}

fn mock_workers() -> Vec<WorkerItem> {
    vec![
        WorkerItem {
            id: "worker-a",
            name: "worker-a",
            active_slots: 2,
            total_slots: 4,
            queue_depth: 6,
            status: WorkerStatus::Running,
        },
        WorkerItem {
            id: "worker-b",
            name: "worker-b",
            active_slots: 3,
            total_slots: 4,
            queue_depth: 4,
            status: WorkerStatus::Running,
        },
    ]
}

fn mock_builds() -> Vec<BuildItem> {
    vec![
        BuildItem {
            id: 1,
            hostname: "atlas-01",
            flake: "campground",
            commit: "a38f45fba91d4b0a5d80840c09b0910c70fa013e",
            branch: "main",
            worker_id: "worker-a",
            queued_for: "queued 00:58 ago",
            runtime: Some("02:13"),
            started_by: "mcamp",
            status: BuildStatus::Building,
            summary: "nix build .#nixosConfigurations.atlas-01.config.system.build.toplevel",
        },
        BuildItem {
            id: 2,
            hostname: "luna-02",
            flake: "campground",
            commit: "75c2fbf719ac2654af9f1dc4b773f502f9db515e",
            branch: "main",
            worker_id: "worker-b",
            queued_for: "queued 01:32 ago",
            runtime: None,
            started_by: "scheduler",
            status: BuildStatus::Queued,
            summary: "waiting for free worker slot",
        },
        BuildItem {
            id: 3,
            hostname: "gray",
            flake: "campground",
            commit: "4144fdc0312734c62bc5f4f9f48f5a87e4b3a85f",
            branch: "main",
            worker_id: "worker-a",
            queued_for: "queued 00:29 ago",
            runtime: None,
            started_by: "scheduler",
            status: BuildStatus::Queued,
            summary: "waiting for free worker slot",
        },
        BuildItem {
            id: 4,
            hostname: "reckless",
            flake: "campground",
            commit: "9cc53a8f1792043b1f7868ecf5ff312ad67553de",
            branch: "release/2026-02",
            worker_id: "worker-b",
            queued_for: "queued 06:11 ago",
            runtime: Some("04:22"),
            started_by: "mcamp",
            status: BuildStatus::Failed,
            summary: "dependency graph diverged on nixpkgs input",
        },
    ]
}

fn mock_logs(build_id: i32) -> Vec<String> {
    let lines = match build_id {
        1 => vec![
            "[10:22:17] systemd[1]: Started crystal-forge-build@atlas-01.service",
            "[10:22:19] CF: reserving build slot worker-a/slot-2",
            "[10:22:22] nix: evaluating flake input graph...",
            "[10:22:26] nix: building /nix/store/5jg9...-kernel-modules.drv",
            "[10:22:31] nix: building /nix/store/qplm...-system-path.drv",
            "[10:22:35] nix: substituter cache hit ratio: 82%",
            "[10:22:41] nix: building /nix/store/nk2p...-etc.drv",
            "[10:22:44] nix: running post-build hooks",
            "[10:22:48] CF: build still running; heartbeat ok",
        ],
        2 => vec![
            "[10:21:02] CF: queued build request for luna-02",
            "[10:21:04] CF: assigned to worker-b queue",
            "[10:21:05] CF: waiting for available slot",
        ],
        3 => vec![
            "[10:21:39] CF: queued build request for gray",
            "[10:21:39] CF: waiting behind 1 queued item",
        ],
        _ => vec![
            "[10:19:11] systemd[1]: Started crystal-forge-build@reckless.service",
            "[10:19:15] nix: evaluating derivation graph",
            "[10:19:44] error: attribute 'myMissingPackage' missing",
            "[10:19:44] CF: build marked failed (exit code 1)",
        ],
    };

    lines.into_iter().map(|line| line.to_string()).collect()
}

fn mock_events(build_id: i32) -> Vec<BuildEvent> {
    match build_id {
        1 => vec![
            BuildEvent {
                ts: "10:22:17",
                level: "info",
                message: "Build unit started on worker-a",
            },
            BuildEvent {
                ts: "10:22:31",
                level: "info",
                message: "Substituter cache hit ratio reached 82%",
            },
            BuildEvent {
                ts: "10:22:48",
                level: "info",
                message: "Worker heartbeat healthy",
            },
        ],
        _ => vec![
            BuildEvent {
                ts: "10:19:11",
                level: "info",
                message: "Build unit started",
            },
            BuildEvent {
                ts: "10:19:44",
                level: "error",
                message: "Nix evaluation failed: missing attribute",
            },
            BuildEvent {
                ts: "10:19:44",
                level: "warn",
                message: "Build marked failed and removed from active worker slot",
            },
        ],
    }
}

fn mock_artifacts(build_id: i32) -> Vec<BuildArtifact> {
    match build_id {
        4 => vec![],
        _ => vec![
            BuildArtifact {
                name: "nixos-system-atlas-01-26.05.20260214.abc123",
                size: "1.3 GiB",
                hash: "sha256-4qkS4W+9Md0v9QY5B5hQmQ8wS6yupw7QmRGYH0xGm4Q=",
            },
            BuildArtifact {
                name: "closure-manifest.json",
                size: "18 KiB",
                hash: "sha256-csY0+fZq0xobLqD7zh9sPXoW3DkQMY8qv5cz4S9xRMo=",
            },
        ],
    }
}
