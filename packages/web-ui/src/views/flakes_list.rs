//! Flakes list view with table/card toggle.

use std::collections::{HashMap, HashSet};
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

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

use crate::alerts::{
    NAV_BADGES, acknowledge_locally, acknowledge_with_cursor_and_ids_async, attention_row_class,
    dismiss_attention_item, occurrence_id_for_subject, should_flash,
};
use crate::api::client::{
    ApiClientError, accept_flake_history_rewrite, create_flake, delete_flake,
    delete_flake_credentials, fetch_commit_diff, fetch_cve_scan_status, fetch_environments,
    fetch_flake_credentials, fetch_flake_module_declarations, fetch_flake_revision_outputs,
    fetch_flake_timeline_for_tray, fetch_flakes, put_flake_credentials, request_sync_all_flakes,
    request_sync_flake, test_flake_credentials, trigger_flake_config_cve_scan, update_flake,
};
use crate::api::models::{
    BuildStatus as ApiBuildStatus, CreateFlakeCredentialRequest, CreateFlakeRequest,
    EnvironmentSummary, FlakeCommitSystemPath, FlakeModuleDeclarationsPage, FlakeOutputDelta,
    FlakeOutputInput, FlakeOutputModule, FlakeOutputPayload, FlakeOutputSnapshotResponse,
    FlakeRegistryItem, FlakeSummary, FlakeSystemFilter, FlakeTimeline, ReconciledFlakeSystem,
    ReconciledFlakeSystemState, SnapshotLifecycle, TestFlakeCredentialRequest, UpdateFlakeRequest,
};
use crate::components::flake::FlakeSyncErrorBanner;
use crate::components::icon::{Icon, IconName};
use crate::components::layout::Card;
use crate::components::notifications::{AlertBanner, AlertSeverity};
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::state::navigation_focus::{
    FlakeNavigation, FlakePane, FocusTarget, NavigationFocus, current_query, update_query,
};
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

fn stable_dom_id(prefix: &str, value: &str) -> String {
    let suffix = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("{prefix}-{suffix}")
}

fn focus_element_by_id(id: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(id))
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = element.focus();
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = id;
    }
}

fn trap_dialog_focus(event: &KeyboardEvent, root_id: &str) {
    if event.key() != Key::Tab {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let Some(root) = document.get_element_by_id(root_id) else {
            return;
        };
        let Ok(nodes) = root.query_selector_all(
            "button:not([disabled]),a[href],input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex='-1'])",
        ) else {
            return;
        };
        let focusable = (0..nodes.length())
            .filter_map(|index| nodes.item(index))
            .filter_map(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
            .collect::<Vec<_>>();
        let (Some(first), Some(last)) = (focusable.first(), focusable.last()) else {
            return;
        };
        let active = document.active_element();
        let root_is_active = active.as_ref() == Some(&root);
        let wraps_backward = event.modifiers().shift()
            && (root_is_active || active.as_ref() == Some(first.as_ref()));
        let wraps_forward = !event.modifiers().shift()
            && (root_is_active || active.as_ref() == Some(last.as_ref()));
        if wraps_backward || wraps_forward {
            event.prevent_default();
            let _ = if wraps_backward {
                last.focus()
            } else {
                first.focus()
            };
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = root_id;
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
    description: String,
    auto_sync: bool,
    sync_interval: String,
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
    environments: Vec<String>,
    description: String,
    auto_sync: bool,
    sync_interval: String,
    build_scope: String,
    credential_type: String,
    credential_username: String,
    credential_secret: String,
    credential_ssh_username: String,
    has_existing_secret: bool,
}

// Legacy FlakesListView and related legacy components removed in TASK-297 cleanup.

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
                h3 { class: "text-lg font-semibold text-white mb-1", "Add flake" }
                p { class: "text-sm {theme::text::SECONDARY} mb-4", "Register a new NixOS flake repository." }
                div {
                    class: "space-y-4",
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
                        label {
                            class: "space-y-2 block md:col-span-2",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Description" }
                            input {
                                class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{draft.read().description}",
                                placeholder: "Short description shown in the registry",
                                oninput: move |evt| {
                                    let mut next = draft.read().clone();
                                    next.description = evt.value();
                                    draft.set(next);
                                },
                            }
                        }
                        div { class: "md:col-span-2", style: "display: grid; grid-template-columns: 1fr 1fr; gap: 14px; padding: 12px; border: 1px solid var(--cf-divider); border-radius: 10px; background: color-mix(in oklab, var(--cf-page-bg) 45%, var(--cf-card-bg));",
                            label { style: "display: flex; gap: 8px; align-items: center; font-size: 13px; cursor: pointer;",
                                input {
                                    r#type: "checkbox",
                                    checked: draft.read().auto_sync,
                                    oninput: move |evt| {
                                        let mut next = draft.read().clone();
                                        next.auto_sync = evt.checked();
                                        draft.set(next);
                                    },
                                    style: "accent-color: var(--cf-brand-purple);"
                                }
                                span { "Auto-sync" }
                            }
                            div { class: "field",
                                label { "Sync interval" }
                                select {
                                    class: "input focus-ring",
                                    value: "{draft.read().sync_interval}",
                                    disabled: !draft.read().auto_sync,
                                    onchange: move |evt| {
                                        let mut next = draft.read().clone();
                                        next.sync_interval = evt.value();
                                        draft.set(next);
                                    },
                                    option { value: "1m", "Every 1 min" }
                                    option { value: "5m", "Every 5 min" }
                                    option { value: "15m", "Every 15 min" }
                                    option { value: "1h", "Every hour" }
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
    let mut confirm_text = use_signal(|| String::new());
    let mut deleting = use_signal(|| false);
    let can_proceed = confirm_text.read().trim() == flake_name;

    rsx! {
        div {
            class: "modal-backdrop",
            style: "z-index: 3300;",
            onclick: move |_| {
                if !deleting() {
                    on_cancel.call(())
                }
            },
            div {
                class: "modal",
                style: "width: min(620px, 96vw); max-height: 92vh;",
                onclick: |evt| evt.stop_propagation(),
                div { class: "modal-head", style: "background: rgba(248,113,113,0.06);",
                    h2 { style: "color: #fecaca; display: flex; align-items: center; gap: 8px;",
                        svg {
                            width: "16",
                            height: "16",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            style: "color: #f87171;",
                            path { d: "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" }
                            path { d: "M12 9v4" }
                            path { d: "M12 17h.01" }
                        }
                        "Remove flake from registry"
                    }
                    p { "This stops sync for " span { class: "mono", style: "font-weight: 600;", "{flake_name}" } " and removes it from the registry." }
                }
                div { class: "modal-body",
                    div {
                        class: "sd-callout sd-callout-danger",
                        style: "flex-direction: column; align-items: stretch;",
                        div { style: "display: flex; gap: 10px; align-items: flex-start;",
                            svg {
                                width: "14",
                                height: "14",
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
                            div { style: "font-size: 12px;",
                                div { style: "font-weight: 600; color: #fecaca; margin-bottom: 4px;", "What happens" }
                                ul { style: "margin: 0; padding-left: 18px; color: var(--cf-text-secondary); line-height: 1.6;",
                                    li { "Sync polling for this flake stops immediately" }
                                    li { "{system_count} system" if system_count != 1 { "s" } " on this flake may need to be retargeted" }
                                    li { "Tracked commits are retained for audit where the backend supports soft delete" }
                                    li { "Repository credentials are not deleted by this action" }
                                }
                            }
                        }
                    }
                    div { class: "field",
                        label { "Type " span { class: "mono", style: "color: #fecaca; font-weight: 700;", "{flake_name}" } " to confirm" }
                        input {
                            class: "input focus-ring mono",
                            placeholder: "{flake_name}",
                            value: "{confirm_text}",
                            autofocus: true,
                            style: if !confirm_text.read().is_empty() && !can_proceed { "border-color: rgba(248,113,113,0.5);" } else { "" },
                            oninput: move |evt| confirm_text.set(evt.value())
                        }
                    }
                }
                div { class: "modal-foot",
                    button {
                        class: "btn btn-ghost focus-ring",
                        disabled: deleting(),
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "btn focus-ring",
                        style: if can_proceed && !deleting() { "background: #dc2626; color: white;" } else { "background: var(--cf-subtle-bg); color: var(--cf-text-muted);" },
                        disabled: !can_proceed || deleting(),
                        onclick: move |_| {
                            deleting.set(true);
                            on_confirm.call((false, false));
                        },
                        if deleting() {
                            "Removing..."
                        } else {
                            svg {
                                width: "13",
                                height: "13",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                style: "margin-right: 6px; vertical-align: text-bottom;",
                                path { d: "M18 6 6 18M6 6l12 12" }
                            }
                            "Remove flake"
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

    let mut draft_signal = use_signal(|| draft.clone());
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
                        div { class: "field",
                            label { "Environments" }
                            div { style: "display: flex; align-items: center; min-height: 34px; gap: 6px; flex-wrap: wrap;",
                                if draft.environments.is_empty() {
                                    span { style: "font-size: 12px; color: var(--cf-text-muted);", "None assigned" }
                                } else {
                                    for env in draft.environments.iter().take(6) {
                                        {
                                            let color_hex = environments.iter()
                                                .find(|e| e.name.eq_ignore_ascii_case(env))
                                                .map(|e| e.color_hex.clone());
                                            rsx! { EnvBadgeNew { env: env.clone(), color_hex } }
                                        }
                                    }
                                    if draft.environments.len() > 6 {
                                        span { class: "chip chip-unknown", style: "font-size: 10px;",
                                            "+{draft.environments.len() - 6}"
                                        }
                                    }
                                }
                            }
                            div { class: "help", "Derived from the systems built off this flake — not assigned here." }
                        }
                    }

                    div { class: "field",
                        label { "Description" }
                        input {
                            class: "input focus-ring",
                            value: "{draft.description}",
                            placeholder: "Short description shown in the registry",
                            oninput: move |evt| {
                                let mut next = draft_signal.read().clone();
                                next.description = evt.value();
                                draft_signal.set(next.clone());
                                on_change.call(next);
                            },
                        }
                    }

                    div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 14px;",
                        label { style: "display: flex; gap: 8px; align-items: center; font-size: 13px; cursor: pointer;",
                            input {
                                r#type: "checkbox",
                                checked: draft.auto_sync,
                                oninput: move |evt| {
                                    let mut next = draft_signal.read().clone();
                                    next.auto_sync = evt.checked();
                                    draft_signal.set(next.clone());
                                    on_change.call(next);
                                },
                                style: "accent-color: var(--cf-brand-purple);"
                            }
                            span { "Auto-sync" }
                        }
                        div { class: "field",
                            label { "Sync interval" }
                            select {
                                class: "input focus-ring",
                                value: "{draft.sync_interval}",
                                disabled: !draft.auto_sync,
                                onchange: move |evt| {
                                    let mut next = draft_signal.read().clone();
                                    next.sync_interval = evt.value();
                                    draft_signal.set(next.clone());
                                    on_change.call(next);
                                },
                                option { value: "1m", "Every 1 min" }
                                option { value: "5m", "Every 5 min" }
                                option { value: "15m", "Every 15 min" }
                                option { value: "1h", "Every hour" }
                            }
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
                SshCredSection {
                    ssh_username: credential_ssh_username.clone(),
                    credential_secret: credential_secret.clone(),
                    has_existing_secret,
                    on_change: on_change.clone(),
                }
            }

            if credential_type == "pat" || credential_type == "username_password" {
                HttpsCredSection {
                    credential_username: credential_username.clone(),
                    credential_secret: credential_secret.clone(),
                    has_existing_secret,
                    on_change: on_change.clone(),
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

/// SSH credential sub-section: shows a preview card when a key already exists,
/// with a "Replace key" toggle to reveal the paste-in form.
#[component]
fn SshCredSection(
    ssh_username: String,
    credential_secret: String,
    has_existing_secret: bool,
    on_change: EventHandler<(String, String)>,
) -> Element {
    let mut replacing = use_signal(|| !has_existing_secret);

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 10px;",
            // Preview card — only when a stored key exists and not currently replacing
            if has_existing_secret && !*replacing.read() {
                div {
                    style: "padding: 10px 12px; border: 1px solid var(--cf-divider); border-radius: 8px; font-size: 11px;",
                    div {
                        style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 4px;",
                        span {
                            class: "mono",
                            style: "font-weight: 600;",
                            if ssh_username.trim().is_empty() { "git" } else { "{ssh_username}" }
                            " — stored SSH key"
                        }
                        button {
                            class: "btn btn-ghost focus-ring xs",
                            onclick: move |_| replacing.set(true),
                            "Replace key"
                        }
                    }
                    div { style: "color: var(--cf-text-muted);",
                        span { class: "mono", "•••• •••• •••• ••••" }
                        " · Encrypted at rest"
                    }
                }
            } else {
                // Add / replace form
                div { style: "display: grid; grid-template-columns: auto 1fr; gap: 10px; align-items: center;",
                    span { style: "font-size: 13px; color: var(--cf-text-secondary);", "SSH username" }
                    input {
                        class: "input focus-ring",
                        value: "{ssh_username}",
                        placeholder: "git",
                        oninput: move |evt| on_change.call(("credential_ssh_username".to_string(), evt.value())),
                    }
                }
                div { class: "field",
                    span { "Private key" }
                    textarea {
                        class: "input focus-ring mono",
                        rows: "5",
                        value: "{credential_secret}",
                        placeholder: "-----BEGIN OPENSSH PRIVATE KEY-----\n…\n-----END OPENSSH PRIVATE KEY-----",
                        style: "font-size: 11px; font-family: var(--font-mono); resize: vertical; padding: 10px;",
                        oninput: move |evt| on_change.call(("credential_secret".to_string(), evt.value())),
                    }
                    div { class: "help", "Encrypted at rest. Crystal Forge never logs key material." }
                }
                if has_existing_secret {
                    div { style: "display: flex; justify-content: flex-end;",
                        button {
                            class: "btn btn-ghost focus-ring xs",
                            onclick: move |_| {
                                replacing.set(false);
                                on_change.call(("credential_secret".to_string(), String::new()));
                            },
                            "Cancel"
                        }
                    }
                }
            }
        }
    }
}

/// HTTPS/PAT credential sub-section: shows a preview card when a token already
/// exists, with a "Replace token" toggle to reveal the input form.
#[component]
fn HttpsCredSection(
    credential_username: String,
    credential_secret: String,
    has_existing_secret: bool,
    on_change: EventHandler<(String, String)>,
) -> Element {
    let mut replacing = use_signal(|| !has_existing_secret);

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 10px;",
            if has_existing_secret && !*replacing.read() {
                div {
                    style: "padding: 10px 12px; border: 1px solid var(--cf-divider); border-radius: 8px; font-size: 11px;",
                    div {
                        style: "display: flex; justify-content: space-between; align-items: center;",
                        span {
                            class: "mono",
                            style: "font-weight: 600;",
                            "•••••••••••••••••••"
                        }
                        button {
                            class: "btn btn-ghost focus-ring xs",
                            onclick: move |_| replacing.set(true),
                            "Replace token"
                        }
                    }
                    if !credential_username.trim().is_empty() {
                        div { style: "margin-top: 4px; color: var(--cf-text-muted);",
                            "User: "
                            span { class: "mono", "{credential_username}" }
                        }
                    }
                }
            } else {
                div { style: "display: grid; grid-template-columns: 1fr 2fr; gap: 10px;",
                    div { class: "field",
                        label { "Username" }
                        input {
                            class: "input focus-ring",
                            value: "{credential_username}",
                            placeholder: "ops-bot",
                            oninput: move |evt| on_change.call(("credential_username".to_string(), evt.value())),
                        }
                    }
                    div { class: "field",
                        label { "Token / password" }
                        input {
                            class: "input focus-ring mono",
                            r#type: "password",
                            value: "{credential_secret}",
                            placeholder: "glpat-… or ghp_…",
                            style: "font-size: 12px;",
                            oninput: move |evt| on_change.call(("credential_secret".to_string(), evt.value())),
                        }
                    }
                }
                if has_existing_secret {
                    div { style: "display: flex; justify-content: flex-end;",
                        button {
                            class: "btn btn-ghost focus-ring xs",
                            onclick: move |_| {
                                replacing.set(false);
                                on_change.call(("credential_secret".to_string(), String::new()));
                            },
                            "Cancel"
                        }
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
            environments: flake.environments.clone(),
            description: String::new(),
            auto_sync: true,
            sync_interval: "5m".to_string(),
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

fn query_param(name: &str) -> Option<String> {
    let window = window()?;
    let search = window.location().search().ok()?;
    let query = search.trim_start_matches('?');
    if query.is_empty() {
        return None;
    }

    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or_default();
        let value = parts.next().unwrap_or_default();
        if key == name {
            return js_sys::decode_uri_component(value)
                .ok()
                .map(|v| v.as_string().unwrap_or_default());
        }
    }

    None
}

/// Remove one or more query parameters from the URL without reloading the page.
fn clear_url_params(names: &[&str]) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(win) = window() else { return };
        let pathname = win.location().pathname().ok().unwrap_or_default();
        let search = win.location().search().ok().unwrap_or_default();
        let query = search.trim_start_matches('?');
        if query.is_empty() {
            return;
        }
        let remaining: Vec<&str> = query
            .split('&')
            .filter(|pair| {
                let key = pair.splitn(2, '=').next().unwrap_or("");
                !names.iter().any(|n| *n == key)
            })
            .collect();
        let new_search = if remaining.is_empty() {
            String::new()
        } else {
            format!("?{}", remaining.join("&"))
        };
        if let Ok(history) = win.history() {
            let _ = history.replace_state_with_url(
                &wasm_bindgen::JsValue::NULL,
                "",
                Some(&format!("{pathname}{new_search}")),
            );
        }
    }
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
        ApiClientError::Status { code, body } => {
            let body_lower = body.to_ascii_lowercase();
            let rewrite_marker = body_lower.contains("history rewrite")
                || body_lower.contains("history_rewrite_detected");

            // The backend ALWAYS returns 409 with the history_rewrite_detected
            // marker for a genuine divergence (see is_history_rewrite_error in
            // flake/commits.rs, which routes to a 409 CONFLICT response).
            // Generic sync failures (network errors, missing/invalid
            // credentials, "Failed to initialize commits for <url>", etc.)
            // return 500 with a message formatted as "Failed to sync {name}
            // from source: {err}" — that message ALWAYS contains the
            // substring "failed to sync", so previously matching on it for
            // any 500 response misclassified every generic sync failure as
            // a rewrite conflict. That caused the "Accept rewrite and
            // resync" flow to repeatedly purge good commit history and
            // re-show the modal in a loop without ever fixing the real
            // underlying error. Only trust the explicit 409 + marker signal.
            if *code != 409 || !rewrite_marker {
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

    fn declaration_page(
        offset: usize,
        total: i64,
        token: &str,
        paths: &[&str],
    ) -> FlakeModuleDeclarationsPage {
        FlakeModuleDeclarationsPage {
            lifecycle: SnapshotLifecycle::Available,
            revision: "a".repeat(40),
            module_name: "large".into(),
            error: None,
            snapshot_token: Some(token.into()),
            total,
            offset,
            limit: 100,
            declarations: paths
                .iter()
                .map(|path| crate::api::models::FlakeModuleDeclaration {
                    path: (*path).into(),
                    declared_type: "string".into(),
                    has_default: false,
                    default: None,
                    source_paths: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn module_declaration_pages_append_without_duplicates_or_skips() {
        let first = declaration_page(0, 3, "digest", &["a", "b"]);
        let second = declaration_page(2, 3, "digest", &["c"]);
        let merged = merge_module_declaration_pages(first, second)
            .expect("contiguous pages from one snapshot should merge");
        assert_eq!(
            merged
                .declarations
                .iter()
                .map(|declaration| declaration.path.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn module_declaration_pages_reject_stale_overlapping_or_skipped_pages() {
        let first = declaration_page(0, 4, "digest", &["a", "b"]);
        assert!(
            merge_module_declaration_pages(
                first.clone(),
                declaration_page(2, 4, "replacement", &["c"]),
            )
            .is_err()
        );
        assert!(
            merge_module_declaration_pages(
                first.clone(),
                declaration_page(3, 4, "digest", &["d"]),
            )
            .is_err()
        );
        assert!(
            merge_module_declaration_pages(first, declaration_page(2, 4, "digest", &["b"]))
                .is_err()
        );
    }

    #[test]
    fn module_declaration_page_zero_requires_exact_identity_and_coherent_metadata() {
        let revision = "a".repeat(40);
        let page = declaration_page(0, 2, "opaque-token", &["a", "b"]);
        assert!(validate_module_declaration_page_zero(page.clone(), &revision, "large").is_ok());

        let mut malformed = page.clone();
        malformed.module_name = "other".into();
        assert!(validate_module_declaration_page_zero(malformed, &revision, "large").is_err());
        let mut malformed = page.clone();
        malformed.snapshot_token = Some("  ".into());
        assert!(validate_module_declaration_page_zero(malformed, &revision, "large").is_err());
        let mut malformed = page.clone();
        malformed.total = 1;
        assert!(validate_module_declaration_page_zero(malformed, &revision, "large").is_err());
        let mut malformed = page;
        malformed.offset = 1;
        assert!(validate_module_declaration_page_zero(malformed, &revision, "large").is_err());
    }

    #[test]
    fn flake_snapshot_poll_backoff_is_bounded() {
        assert_eq!(flake_snapshot_poll_delay_ms(0), 3_000);
        assert_eq!(flake_snapshot_poll_delay_ms(1), 6_000);
        assert_eq!(flake_snapshot_poll_delay_ms(2), 12_000);
        assert_eq!(flake_snapshot_poll_delay_ms(20), 12_000);
    }

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

    #[test]
    fn displayed_sha_prefix_never_aliases_full_commit_identity() {
        use crate::api::models::FlakeCommit;

        let shared_prefix = "abcdef0";
        let first_sha = format!("{shared_prefix}111111111111111111111111111111111");
        let second_sha = format!("{shared_prefix}222222222222222222222222222222222");
        let committed_at = Utc::now();
        let commits = vec![
            FlakeCommit {
                id: 1,
                hash: first_sha.clone(),
                message: "first".to_string(),
                author: "Alice".to_string(),
                committed_at,
                system_count: 0,
                commits_behind: 0,
                systems: vec![],
                system_paths: vec![],
                build_status: None,
                evaluation_status: None,
                evaluation_error_message: None,
            },
            FlakeCommit {
                id: 2,
                hash: second_sha.clone(),
                message: "second".to_string(),
                author: "Bob".to_string(),
                committed_at,
                system_count: 0,
                commits_behind: 0,
                systems: vec![],
                system_paths: vec![],
                build_status: None,
                evaluation_status: None,
                evaluation_error_message: None,
            },
        ];

        let mapped = map_timeline_commits_to_view(&commits);
        assert_eq!(mapped[0].sha, shared_prefix);
        assert_eq!(mapped[1].sha, shared_prefix);
        assert!(commit_has_full_sha(&mapped[0], &first_sha));
        assert!(!commit_has_full_sha(&mapped[0], &second_sha));
        assert!(commit_has_full_sha(&mapped[1], &second_sha));
        assert!(!commit_has_full_sha(&mapped[1], &first_sha));
    }

    #[test]
    fn extract_history_rewrite_conflict_matches_genuine_409_marker() {
        let body = "Git history rewrite detected for boterf-config. Review and accept rewrite before sync.".to_string();
        let error = ApiClientError::Status {
            code: 409,
            body: body.clone(),
        };
        assert_eq!(
            extract_history_rewrite_conflict(&error, Some(4)),
            Some((4, body))
        );
    }

    #[test]
    fn extract_history_rewrite_conflict_ignores_generic_500_sync_failure() {
        // Regression test: a generic sync failure (e.g. network/credentials
        // issue) is always formatted as "Failed to sync {name} from source:
        // {err}" by the backend, which contains the substring "failed to
        // sync" but is NOT a real history rewrite. Misclassifying this
        // caused "Accept rewrite and resync" to loop forever, repeatedly
        // purging good commit history without ever fixing the real error.
        let error = ApiClientError::Status {
            code: 500,
            body: "Failed to sync boterf-config from source: Failed to initialize commits for https://gitlab.com/michaelboterf/nix-configurations".to_string(),
        };
        assert_eq!(extract_history_rewrite_conflict(&error, Some(4)), None);
    }

    #[test]
    fn extract_history_rewrite_conflict_ignores_marker_text_on_non_409_status() {
        // Even if a 500 response happens to mention "history rewrite" in
        // free text, only the backend's canonical 409 CONFLICT response is
        // trusted as a genuine divergence signal.
        let error = ApiClientError::Status {
            code: 500,
            body: "unexpected error while checking history rewrite state".to_string(),
        };
        assert_eq!(extract_history_rewrite_conflict(&error, Some(4)), None);
    }

    #[test]
    fn registry_mapping_does_not_fabricate_environment_badges() {
        let item = FlakeRegistryItem {
            id: 7,
            name: "platform-core".to_string(),
            repo_url: "https://gitlab.com/crystal-forge/platform-core.git".to_string(),
            branch: "main".to_string(),
            build_scope: "cf_systems_only".to_string(),
            system_count: 3,
            sync_status: "synced".to_string(),
            last_sync_at: None,
            last_sync_error: None,
            // TASK-397 enriched fields — empty for a flake with no commits/environments
            latest_commit_hash: None,
            latest_commit_message: None,
            latest_commit_author: None,
            latest_commit_timestamp: None,
            build_status: None,
            evaluation_status: None,
            environments: Vec::new(),
            total_commit_count: 0,
        };

        let mapped = map_registry_flake_to_view(&item);

        // With no environments in the registry response, environment is empty.
        assert_eq!(mapped.environment, "");
        assert_eq!(mapped.description, "Build scope: cf_systems_only");
    }

    #[test]
    fn output_payload_helpers_keep_authoritative_revision_data() {
        let snapshot: FlakeOutputSnapshotResponse = serde_json::from_value(serde_json::json!({
            "lifecycle": "available", "revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "first_parent_revision": null, "first_parent_resolved": true,
            "comparison_available": false, "error": null,
            "snapshot_token": "1".repeat(64),
            "outputs": {
                "declared_systems": ["web", "db"],
                "exported_modules": [{"name": "default", "description": null, "source_input": "self", "source_revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "source_path": "flake.nix", "declarations": [], "consumers": [], "declaration_count": 0, "consumer_count": 0, "error": null}],
                "inputs": [
                    {"node": "flake-utils", "names": [], "direct": false, "transitive": true, "follows": [], "original": {}, "locked": {}, "source_type": "github", "source": "github:numtide/flake-utils", "locked_revision": "dddddddddddddddddddddddddddddddddddddddd", "last_modified": 2, "channel": false, "tracked": true, "direct_descendant_count": null, "transitive_descendant_count": null},
                    {"node": "nixpkgs", "names": ["nixpkgs"], "direct": true, "transitive": false, "follows": [], "original": {}, "locked": {}, "source_type": "github", "source": "github:NixOS/nixpkgs", "locked_revision": "cccccccccccccccccccccccccccccccccccccccc", "last_modified": 1, "channel": false, "tracked": true, "direct_descendant_count": 1, "transitive_descendant_count": 1}
                ],
                "direct_input_count": 1, "resolved_input_count": 2, "lock_error": null,
                "module_evaluation": {"available": true, "source": "nixpkgs", "error": null},
                "nixpkgsRevisions": ["cccccccccccccccccccccccccccccccccccccccc"],
                "multiple_nixpkgs_revisions": false
            },
            "previous_outputs": null, "delta": null, "systems": [],
            "managed_system_count": 0, "declared_system_count": 2,
            "previous_declared_system_count": null,
            "declared_unmanaged_count": 2, "managed_undeclared_count": 0,
            "output_collapsed_count": 0, "pinned_revision_count": 0,
            "stale_direct_input_count": 1,
            "exported_module_count": 1,
            "pagination": {"offset": 0, "limit": 100, "system_total": 0, "systems_has_more": false}
        })).expect("typed output payload");

        assert_eq!(flake_output_modules(&snapshot)[0].name, "default");
        assert_eq!(
            flake_output_inputs(&snapshot)[0].locked_revision.as_deref(),
            Some("cccccccccccccccccccccccccccccccccccccccc")
        );
        assert_eq!(authoritative_input_count(Some(&snapshot)), 1);
        let inputs = flake_output_inputs(&snapshot);
        assert_eq!(inputs.len(), 2);
        assert!(inputs[0].direct);
        assert!(inputs[1].transitive);
        assert_eq!(authoritative_system_reconciliation_count(&snapshot), 2);
        assert_eq!(snapshot.exported_module_count, 1);
    }

    #[test]
    fn output_collection_pages_merge_without_replacing_authoritative_totals() {
        let page = |offset, name| {
            serde_json::from_value::<FlakeOutputSnapshotResponse>(serde_json::json!({
                "lifecycle": "available", "revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "first_parent_revision": null, "first_parent_resolved": true,
                "comparison_available": false, "error": null,
                "snapshot_token": "2".repeat(64),
                "outputs": {
                    "declared_systems": [name], "exported_modules": [], "inputs": [],
                    "direct_input_count": 0, "resolved_input_count": 0, "lock_error": null,
                    "module_evaluation": {"available": true, "source": "nixpkgs", "error": null},
                    "nixpkgsRevisions": [], "multiple_nixpkgs_revisions": false
                },
                "previous_outputs": null, "delta": null,
                "systems": [{"configuration_name": name, "system_id": null, "hostname": null, "environment_name": null, "environment_color": null, "state": "declared_unmanaged", "deployed_revision": null, "output_collapsed": false}],
                "managed_system_count": 7, "declared_system_count": 2,
                "previous_declared_system_count": null,
                "declared_unmanaged_count": 2, "managed_undeclared_count": 1,
                "output_collapsed_count": 3, "pinned_revision_count": 4,
                "stale_direct_input_count": 2,
                "exported_module_count": 5,
                "pagination": {"offset": offset, "limit": 1, "system_total": 2, "systems_has_more": offset == 0}
            }))
            .expect("output page")
        };
        let first = page(0, "web");
        let second = page(1, "db");

        assert!(flake_output_pane_has_more(FlakePane::Systems, &first));
        assert!(flake_output_pane_has_more(FlakePane::Modules, &first));
        assert!(!flake_output_pane_has_more(FlakePane::Inputs, &first));
        let mut replacement = second.clone();
        replacement.snapshot_token = Some("3".repeat(64));
        assert!(merge_flake_output_pages(first.clone(), replacement).is_err());
        let merged = merge_flake_output_pages(first, second).expect("same snapshot pages");
        assert_eq!(merged.managed_system_count, 7);
        assert_eq!(merged.declared_system_count, 2);
        assert_eq!(merged.exported_module_count, 5);
        assert_eq!(merged.output_collapsed_count, 3);
        assert_eq!(merged.systems.len(), 2);
        assert_eq!(
            merged.outputs.expect("outputs").declared_systems,
            ["web", "db"]
        );
    }

    #[test]
    fn output_page_merge_deduplicates_revision_global_collections() {
        let mut first = serde_json::from_value::<FlakeOutputDelta>(serde_json::json!({
            "systems_added_total": 1, "systems_removed_total": 0,
            "modules_added_total": 1, "modules_removed_total": 0,
            "inputs_added_total": 1, "inputs_removed_total": 0,
            "input_revision_bumps_total": 1,
            "systems_added": ["web"], "systems_removed": [],
            "modules_added": ["base"], "modules_removed": [],
            "inputs_added": ["nixpkgs"], "inputs_removed": [],
            "input_revision_bumps": [{"node": "nixpkgs", "before": "a", "after": "b"}]
        }))
        .expect("first delta");
        let second = first.clone();
        first = merge_flake_output_deltas(Some(first), Some(second)).expect("merged delta");
        assert_eq!(first.systems_added, ["web"]);
        assert_eq!(first.modules_added, ["base"]);
        assert_eq!(first.inputs_added, ["nixpkgs"]);
        assert_eq!(first.input_revision_bumps.len(), 1);
    }

    #[test]
    fn bounded_delta_titles_report_exact_totals_and_omitted_samples() {
        assert_eq!(
            delta_sample_title(&["web".into()], 3),
            "web\n2 more not shown"
        );
        let delta: FlakeOutputDelta = serde_json::from_value(serde_json::json!({
            "systems_added_total": 3, "systems_removed_total": 2,
            "modules_added_total": 0, "modules_removed_total": 0,
            "inputs_added_total": 0, "inputs_removed_total": 0,
            "input_revision_bumps_total": 1,
            "systems_added": ["web"], "systems_removed": ["old"],
            "modules_added": [], "modules_removed": [],
            "inputs_added": [], "inputs_removed": [], "input_revision_bumps": []
        }))
        .expect("bounded delta");
        assert_eq!(flake_delta_total(&delta), 6);
    }

    #[test]
    fn declared_output_collapse_requires_a_strict_first_parent_reduction() {
        assert_eq!(declared_output_collapse(Some(12), 4), Some(12));
        assert_eq!(declared_output_collapse(Some(12), 12), None);
        assert_eq!(declared_output_collapse(Some(12), 14), None);
        assert_eq!(declared_output_collapse(None, 0), None);
    }

    #[test]
    fn follows_strings_and_missing_module_bindings_are_truthful() {
        assert_eq!(
            render_follows_value(&serde_json::json!("nixpkgs")),
            "nixpkgs"
        );
        assert_eq!(
            render_follows_value(&serde_json::json!(["foo", "bar"])),
            "foo/bar"
        );

        let module: FlakeOutputModule = serde_json::from_value(serde_json::json!({
            "name": "base", "description": null, "source_input": null,
            "source_revision": null, "source_path": null, "declarations": [],
            "declarations_complete": false, "consumers": [], "declaration_count": 0,
            "consumer_count": 0, "error": null
        }))
        .expect("module");
        assert_eq!(module_binding_label(&module), "Export binding unavailable");
    }

    #[test]
    fn rollout_denominator_uses_authoritative_managed_fleet_total() {
        let mut commits = mock_commits_for_flake(1);
        apply_managed_rollout_total(&mut commits, 19);
        assert!(commits.iter().all(|commit| commit.rollout_total == 19));
    }

    #[test]
    fn registration_prefill_preserves_configuration_flake_and_branch() {
        let url = registration_prefill_url("host one", "platform/core", "release 1");
        assert!(url.contains("hostname=host%20one"));
        assert!(url.contains("configuration=host%20one"));
        assert!(url.contains("flake_name=platform%2Fcore"));
        assert!(url.contains("branch=release%201"));
    }
}

/// Renders the flake registry, revision panes, and commit diffs.
///
/// `initial_query` preserves tray state parsed by Dioxus during a direct page load.
#[component]
pub fn FlakesListViewNew(initial_query: String) -> Element {
    let nav = navigator();
    let mut navigation_focus = use_context::<Signal<Option<NavigationFocus>>>();
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
        description: String::new(),
        auto_sync: true,
        sync_interval: "5m".to_string(),
        build_scope: "cf_systems_only".to_string(),
        credential_type: "none".to_string(),
        credential_username: String::new(),
        credential_secret: String::new(),
        credential_ssh_username: String::new(),
    });
    let mut rewrite_prompt = use_signal(|| None::<(i32, String, String)>);
    let mut dismissed_rewrite_conflicts = use_signal(HashSet::<String>::new);
    let mut flakes_ack_sent = use_signal(|| false);
    let mut flakes_ack_in_flight = use_signal(|| false);
    let mut flakes_last_ack_attempt_cursor = use_signal(|| None::<String>);
    let mut flakes_local_ack_hidden = use_signal(|| false);
    let initial_search = if initial_query.is_empty() {
        current_query()
    } else {
        format!("?{initial_query}")
    };
    let initial_flake_navigation = FlakeNavigation::from_query(&initial_search);
    let mut flake_navigation = use_signal(|| initial_flake_navigation.clone());
    #[cfg(target_arch = "wasm32")]
    {
        let popstate_listener = use_hook(|| {
            let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                flake_navigation.set(FlakeNavigation::from_query(&current_query()));
            });
            if let Some(window) = web_sys::window() {
                let _ = window.add_event_listener_with_callback(
                    "popstate",
                    callback.as_ref().unchecked_ref(),
                );
            }
            Rc::new(callback)
        });
        let listener_for_drop = popstate_listener.clone();
        use_drop(move || {
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback(
                    "popstate",
                    listener_for_drop.as_ref().as_ref().unchecked_ref(),
                );
            }
        });
    }
    let focus_flake_id = query_param("focus_flake_id").and_then(|value| value.parse::<i32>().ok());
    let legacy_focus_sha = initial_flake_navigation
        .revision
        .clone()
        .or_else(|| query_param("focus_sha"));
    let focus_meta = if legacy_focus_sha.is_some() {
        Some(CommitFocusMeta {
            msg: query_param("focus_msg"),
            author: query_param("focus_author"),
            at: query_param("focus_at"),
        })
    } else {
        None
    };

    // TASK-397: Enriched registry fetch — single request for all initial data.
    // The registry now returns latest commit summary, environments, and total
    // commit count so separate timeline and systems requests are not needed.
    let flakes_resource = use_resource(move || {
        let _nonce = *reload_nonce.read();
        async move { fetch_flakes().await }
    });
    // Environments are loaded eagerly on mount so the table/cards
    // "Environments" column can render each pill with its real configured
    // color (Environment.color_hex) from first paint, matching the pattern
    // used by the Systems view (`environment_colors_resource` in
    // systems_list.rs). Previously this was gated behind the add/edit
    // dialog opening, so on a normal page load `db_environments` was empty
    // and every pill silently fell back to a hardcoded 4-name palette that
    // ignores the environment's actual color for any name outside that
    // fixed set.
    let environments_resource = use_resource(move || async move { fetch_environments().await });
    let db_environments: Vec<EnvironmentSummary> = match environments_resource.read().as_ref() {
        Some(Ok(envs)) => envs.clone(),
        _ => Vec::new(),
    };

    let (raw_flakes, load_error, loading) = match flakes_resource.read().as_ref() {
        Some(Ok(items)) => (items.clone(), None, false),
        Some(Err(err)) => (Vec::new(), Some(err.to_string()), false),
        None => (Vec::new(), None, true),
    };

    // Map directly from the enriched registry — no secondary requests needed.
    let all_flakes: Vec<MockFlakeItem> =
        raw_flakes.iter().map(map_registry_flake_to_view).collect();

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
    let total_flake_count = all_flakes.len();
    let total_systems: i32 = all_flakes.iter().map(|f| f.system_count).sum();
    let synced_count = all_flakes.iter().filter(|f| f.status == "synced").count();
    let syncing_count = all_flakes.iter().filter(|f| f.status == "syncing").count();
    let error_count = all_flakes.iter().filter(|f| f.status == "error").count();
    // Acknowledge the "flakes" sidebar badge on first visit and trigger attention
    // flash on errored rows (TASK-385).
    let has_flake_errors = error_count > 0;
    let flash_flakes = should_flash("flakes", has_flake_errors);
    use_effect(move || {
        let flakes_loaded_successfully = matches!(flakes_resource.read().as_ref(), Some(Ok(_)));
        let observed_at = NAV_BADGES.read().observed_at.clone();
        if flakes_loaded_successfully && !flakes_ack_sent() && !flakes_ack_in_flight() {
            if let Some(cursor) = observed_at {
                if flakes_last_ack_attempt_cursor.read().as_deref() == Some(cursor.as_str()) {
                    return;
                }
                let occurrence_ids = NAV_BADGES.read().flakes_occurrence_ids.clone();
                flakes_ack_in_flight.set(true);
                flakes_last_ack_attempt_cursor.set(Some(cursor.clone()));
                spawn(async move {
                    let success =
                        acknowledge_with_cursor_and_ids_async("flakes", cursor, occurrence_ids)
                            .await;
                    flakes_ack_in_flight.set(false);
                    if success {
                        flakes_ack_sent.set(true);
                    }
                });
            } else if !flakes_local_ack_hidden() {
                flakes_local_ack_hidden.set(true);
                acknowledge_locally("flakes");
            }
        }
    });

    let selected_flake_value = selected_flake.read().clone();
    let selected_flake_for_timeline = selected_flake.clone();
    // TASK-397: Lazy per-flake timeline fetch — only triggered when a tray is
    // opened. Never falls back to the all-flakes timeline endpoint; a failed
    // request surfaces a tray-local error instead.
    let selected_timeline_resource = use_resource(move || {
        let flake_id = selected_flake_for_timeline.read().as_ref().map(|f| f.id);
        let _nonce = *reload_nonce.read();
        async move {
            match flake_id {
                Some(id) => fetch_flake_timeline_for_tray(id).await,
                None => Ok(Vec::new()),
            }
        }
    });

    {
        let all_flakes = all_flakes.clone();
        let mut selected_flake = selected_flake.clone();
        use_effect(move || {
            let current = selected_flake.read().clone();
            if let Some(active) = current {
                match all_flakes.iter().find(|flake| flake.id == active.id) {
                    // Keep the open tray's sync_status/error/commit fields in sync with
                    // the latest fetch (e.g. after a "Retry sync" completes and
                    // reload_nonce refetches flakes). Without this, the tray kept
                    // showing a stale "Sync failed" banner until the user closed and
                    // reopened it, even after the sync had actually succeeded.
                    Some(fresh) if fresh != &active => {
                        selected_flake.set(Some(fresh.clone()));
                    }
                    Some(_) => {}
                    None => selected_flake.set(None),
                }
            }
        });
    }
    {
        let all_flakes = all_flakes.clone();
        let mut selected_flake = selected_flake.clone();
        let flakes_resource = flakes_resource;
        use_effect(move || {
            let _ = flakes_resource.read();
            let target = flake_navigation.read().clone();
            let target_id = target.flake_id.or(focus_flake_id);
            if target_id.is_none() && target.flake_name.is_none() {
                if selected_flake.read().is_some() {
                    selected_flake.set(None);
                }
                return;
            }
            let matched = all_flakes.iter().find(|flake| {
                target_id == Some(flake.id)
                    || target
                        .flake_name
                        .as_ref()
                        .is_some_and(|name| name.eq_ignore_ascii_case(&flake.name))
            });
            if let Some(flake) = matched {
                if selected_flake.read().as_ref().map(|item| item.id) != Some(flake.id) {
                    selected_flake.set(Some(flake.clone()));
                }
            } else if !all_flakes.is_empty() {
                let cleared = FlakeNavigation::cleared();
                update_query(&cleared.to_query(&current_query()), false);
                flake_navigation.set(cleared);
                selected_flake.set(None);
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
                        "{total_flake_count} tracked · {total_systems} systems · {synced_count} synced"
                    }
                }
                div { style: "display: flex; gap: 8px;",
                    if is_admin_user {
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

            div { class: "stat-strip flakes-stat-strip", "data-testid": "flakes-stat-strip",
                div { class: "stat",
                    div { class: "stat-accent", style: "--stat-color: var(--cf-brand-purple);" }
                    div { class: "stat-label", "Tracked" }
                    div { class: "stat-value", "{flake_count}" }
                    div { class: "stat-meta", "registered flakes" }
                }
                div { class: "stat",
                    div { class: "stat-accent", style: "--stat-color: #60a5fa;" }
                    div { class: "stat-label", "Systems" }
                    div { class: "stat-value", "{total_systems}" }
                    div { class: "stat-meta", "mapped hosts" }
                }
                div { class: "stat",
                    div { class: "stat-accent", style: "--stat-color: #34d399;" }
                    div { class: "stat-label", "Synced" }
                    div { class: "stat-value", "{synced_count}" }
                    div { class: "stat-meta", "latest status clean" }
                }
                div { class: "stat",
                    div { class: "stat-accent", style: "--stat-color: #f59e0b;" }
                    div { class: "stat-label", "Syncing" }
                    div { class: "stat-value", "{syncing_count}" }
                    div { class: "stat-meta", "queued or building" }
                }
                div { class: "stat",
                    div { class: "stat-accent", style: "--stat-color: #f87171;" }
                    div { class: "stat-label", "Errors" }
                    div { class: "stat-value", "{error_count}" }
                    div { class: "stat-meta", "needs attention" }
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

            if *show_add_form.read() {
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
                                            description: String::new(),
                                            auto_sync: true,
                                            sync_interval: "5m".to_string(),
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
                        let all_flakes_for_edit = all_flakes.clone();
                        rsx! { FlakeTableNew { flakes: filtered_flakes.clone(), selected_id, is_admin: is_admin_user, env_colors: db_environments.clone(), flash_errors: flash_flakes, on_select: move |f: MockFlakeItem| {
                            let next = FlakeNavigation { flake_id: Some(f.id), ..Default::default() };
                            update_query(&next.to_query(&current_query()), true);
                            flake_navigation.set(next);
                            selected_flake.set(Some(f));
                        }, on_sync: move |flake_id| {
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
                                            maybe_set_rewrite_prompt(
                                                &mut rewrite_prompt,
                                                &dismissed_rewrite_conflicts,
                                                id,
                                                detail,
                                            );
                                            action_notice.set(Some("Sync blocked: git history rewrite detected. Review and accept rewrite to continue.".to_string()));
                                        } else {
                                            action_notice.set(Some(format!("Sync failed: {err}")));
                                        }
                                    }
                                }
                            });
                        }, on_edit: move |flake_id| {
                            if let Some(current) = all_flakes_for_edit.iter().find(|item| item.id == flake_id) {
                                let base_draft = EditFlakeDraft {
                                    id: current.id,
                                    name: current.name.clone(),
                                    repo_url: current.url.clone(),
                                    branch: current.branch.clone(),
                                    environments: current.environment.split(',').map(str::trim).filter(|s| !s.is_empty()).map(ToString::to_string).collect(),
                                    description: current.description.clone(),
                                    auto_sync: true,
                                    sync_interval: "5m".to_string(),
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
                        } } }
                    } else {
                        let all_flakes_for_edit = all_flakes.clone();
                        rsx! { FlakeCardsNew { flakes: filtered_flakes.clone(), selected_id, is_admin: is_admin_user, env_colors: db_environments.clone(), flash_errors: flash_flakes, on_select: move |f: MockFlakeItem| {
                            let next = FlakeNavigation { flake_id: Some(f.id), ..Default::default() };
                            update_query(&next.to_query(&current_query()), true);
                            flake_navigation.set(next);
                            selected_flake.set(Some(f));
                        }, on_sync: move |flake_id| {
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
                                            maybe_set_rewrite_prompt(
                                                &mut rewrite_prompt,
                                                &dismissed_rewrite_conflicts,
                                                id,
                                                detail,
                                            );
                                            action_notice.set(Some("Sync blocked: git history rewrite detected. Review and accept rewrite to continue.".to_string()));
                                        } else {
                                            action_notice.set(Some(format!("Sync failed: {err}")));
                                        }
                                    }
                                }
                            });
                        }, on_edit: move |flake_id| {
                            if let Some(current) = all_flakes_for_edit.iter().find(|item| item.id == flake_id) {
                                let base_draft = EditFlakeDraft {
                                    id: current.id,
                                    name: current.name.clone(),
                                    repo_url: current.url.clone(),
                                    branch: current.branch.clone(),
                                    environments: current.environment.split(',').map(str::trim).filter(|s| !s.is_empty()).map(ToString::to_string).collect(),
                                    description: current.description.clone(),
                                    auto_sync: true,
                                    sync_interval: "5m".to_string(),
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
                        } } }
                    }
                }
            }

            // Side tray (if flake selected)
            if let Some(flake) = selected_flake_value {
                {
                    let mut selected_direct_commits = match selected_timeline_resource.read().as_ref() {
                        Some(Ok(items)) => items
                            .iter()
                            .find(|timeline| timeline.flake_id == flake.id)
                            .map(|timeline| map_timeline_commits_to_view(&timeline.commits))
                            .unwrap_or_default(),
                        _ => Vec::new(),
                    };
                    apply_managed_rollout_total(
                        &mut selected_direct_commits,
                        flake.system_count,
                    );
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
                            key: "{flake.id}",
                            commits: tray_commits,
                            commits_loading: tray_commits_loading,
                            commits_error: tray_commits_error,
                            notice: action_notice.read().clone(),
                            is_admin: is_admin_user,
                            focus_sha: flake_navigation.read().revision.clone().or_else(|| legacy_focus_sha.clone()),
                            focus_meta: focus_meta.clone(),
                            initial_pane: flake_navigation.read().pane,
                            flake,
                            on_edit: move |flake_id| {
                                if let Some(current) = all_flakes_for_edit.iter().find(|item| item.id == flake_id) {
                                    let base_draft = EditFlakeDraft {
                                        id: current.id,
                                        name: current.name.clone(),
                                        repo_url: current.url.clone(),
                                        branch: current.branch.clone(),
                                        environments: current.environment.split(',').map(str::trim).filter(|s| !s.is_empty()).map(ToString::to_string).collect(),
                                        description: current.description.clone(),
                                        auto_sync: true,
                                        sync_interval: "5m".to_string(),
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
                                        }
                                        Err(err) => {
                                            if let Some((id, detail)) =
                                                extract_history_rewrite_conflict(&err, Some(flake_id))
                                            {
                                                maybe_set_rewrite_prompt(
                                                    &mut rewrite_prompt,
                                                    &dismissed_rewrite_conflicts,
                                                    id,
                                                    detail,
                                                );
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
                                maybe_set_rewrite_prompt(
                                    &mut rewrite_prompt,
                                    &dismissed_rewrite_conflicts,
                                    flake_id,
                                    detail,
                                );
                                action_notice.set(Some("Sync blocked: git history rewrite detected. Review and accept rewrite to continue.".to_string()));
                            },
                            on_navigation_change: move |(pane, revision, push): (FlakePane, Option<String>, bool)| {
                                let mut next = flake_navigation.read().clone();
                                next.pane = pane;
                                next.revision = revision;
                                update_query(&next.to_query(&current_query()), push);
                                flake_navigation.set(next);
                            },
                            on_close: move |_| {
                                let return_environment = flake_navigation.read().return_environment.clone();
                                if let Some(environment) = return_environment {
                                    // Return directly. Mutating the Flakes query and tray
                                    // state first can schedule another tray render before
                                    // the router unmounts this view.
                                    nav.push(Route::EnvironmentsView {
                                        query: format!("panel={environment}"),
                                    });
                                    return;
                                }
                                let opener_id = selected_flake
                                    .read()
                                    .as_ref()
                                    .map(|flake| format!("flake-opener-{}", flake.id));
                                let cleared = FlakeNavigation::cleared();
                                update_query(&cleared.to_query(&current_query()), true);
                                flake_navigation.set(cleared);
                                selected_flake.set(None);
                                if let Some(opener_id) = opener_id {
                                    focus_element_by_id(&opener_id);
                                }
                            },
                            on_open_evaluation: move |focus: NavigationFocus| {
                                navigation_focus.set(Some(focus));
                                nav.push(Route::EvaluationsView {});
                            },
                            on_open_build: move |focus: NavigationFocus| {
                                navigation_focus.set(Some(focus));
                                nav.push(Route::BuildsView {});
                            },
                            on_open_systems: move |focus: NavigationFocus| {
                                navigation_focus.set(Some(focus));
                                nav.push(Route::SystemsView { query: String::new() });
                            },
                        }
                    }
                }
            }

            if let Some((flake_id, flake_name, detail)) = rewrite_prompt.read().clone() {
                {
                    let detail_for_cancel = detail.clone();
                    let detail_for_accept = detail.clone();
                    rsx! {
                        HistoryRewriteDialog {
                            flake_name,
                            detail,
                            on_cancel: move |_| {
                                dismissed_rewrite_conflicts.with_mut(|set| {
                                    set.insert(rewrite_conflict_key(flake_id, &detail_for_cancel));
                                });
                                rewrite_prompt.set(None)
                            },
                            on_accept: move |_| {
                                let mut rewrite_prompt = rewrite_prompt.clone();
                                let mut action_notice = action_notice.clone();
                                let mut reload_nonce = reload_nonce.clone();
                                let mut dismissed_rewrite_conflicts = dismissed_rewrite_conflicts.clone();
                                let conflict_key = rewrite_conflict_key(flake_id, &detail_for_accept);
                                spawn(async move {
                                    match accept_flake_history_rewrite(flake_id).await {
                                        Ok(response) => {
                                            dismissed_rewrite_conflicts.with_mut(|set| {
                                                set.remove(&conflict_key);
                                            });
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
                                        Err(err) => {
                                            action_notice.set(Some(format!(
                                                "Failed to accept rewrite: {err}"
                                            )));
                                        }
                                    }
                                });
                            }
                        }
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
// View models for the flake registry and revision tray.
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub(crate) struct MockFlakeItem {
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
    last_sync_at_raw: Option<DateTime<Utc>>,
    environment: String,
    error_msg: Option<String>,
    total_commits: i32,
}

impl MockFlakeItem {
    pub(crate) fn id(&self) -> i32 {
        self.id
    }
}

fn map_registry_flake_to_view(item: &FlakeRegistryItem) -> MockFlakeItem {
    let build_scope_label = if item.build_scope.trim().is_empty() {
        "default"
    } else {
        item.build_scope.trim()
    };

    // Map real sync fields from the API (TASK-385).
    let last_sync_display = item
        .last_sync_at
        .as_ref()
        .map(|dt| relative_time_label(*dt))
        .unwrap_or_else(|| "Not synced yet".to_string());

    // TASK-397: Use enriched fields from the registry response.
    // These fields are now returned by GET /api/v1/flakes directly,
    // so no separate timeline or systems request is needed for initial render.
    let latest_commit = item
        .latest_commit_hash
        .as_deref()
        .unwrap_or("—")
        .to_string();
    let latest_message = item
        .latest_commit_message
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("No commits yet")
        .to_string();
    let latest_author = item
        .latest_commit_author
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("—")
        .to_string();
    let environment = item.environments.join(",");
    let total_commits = item.total_commit_count as i32;

    MockFlakeItem {
        id: item.id,
        name: item.name.clone(),
        description: format!("Build scope: {build_scope_label}"),
        status: item.sync_status.clone(),
        url: item.repo_url.clone(),
        branch: item.branch.clone(),
        build_scope: item.build_scope.clone(),
        system_count: item.system_count as i32,
        latest_commit,
        latest_message,
        latest_author,
        last_sync_at: last_sync_display,
        last_sync_at_raw: item.last_sync_at,
        environment,
        error_msg: item.last_sync_error.clone(),
        total_commits,
    }
}

pub(crate) fn map_flake_summary_to_tray_item(summary: &FlakeSummary) -> MockFlakeItem {
    MockFlakeItem {
        id: summary.id,
        name: summary.name.clone(),
        description: "System flake context".to_string(),
        status: "synced".to_string(),
        url: summary.repo_url.clone(),
        branch: "unknown".to_string(),
        build_scope: String::new(),
        system_count: 1,
        latest_commit: summary
            .latest_commit
            .clone()
            .unwrap_or_else(|| "—".to_string()),
        latest_message: "System deployment context".to_string(),
        latest_author: "—".to_string(),
        last_sync_at: "not persisted".to_string(),
        last_sync_at_raw: None,
        environment: String::new(),
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

fn rewrite_conflict_key(flake_id: i32, detail: &str) -> String {
    format!("{flake_id}:{detail}")
}

fn maybe_set_rewrite_prompt(
    rewrite_prompt: &mut Signal<Option<(i32, String, String)>>,
    dismissed_rewrite_conflicts: &Signal<HashSet<String>>,
    flake_id: i32,
    detail: String,
) {
    let key = rewrite_conflict_key(flake_id, &detail);
    if dismissed_rewrite_conflicts.read().contains(&key) {
        return;
    }
    rewrite_prompt.set(Some((flake_id, format!("flake #{flake_id}"), detail)));
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

pub(crate) fn map_timeline_commits_to_view(
    commits: &[crate::api::models::FlakeCommit],
) -> Vec<MockCommitItem> {
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

fn apply_managed_rollout_total(commits: &mut [MockCommitItem], managed_system_count: i32) {
    for commit in commits {
        commit.rollout_total = managed_system_count;
    }
}

fn commit_has_full_sha(commit: &MockCommitItem, full_sha: &str) -> bool {
    !full_sha.is_empty() && commit.full_hash == full_sha
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
            last_sync_at_raw: None,
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
            last_sync_at_raw: None,
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
            last_sync_at_raw: None,
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
pub(crate) struct MockCommitItem {
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

/// Optional metadata for a focused/deployed commit that may not be in the commits list.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct CommitFocusMeta {
    pub msg: Option<String>,
    pub author: Option<String>,
    pub at: Option<String>,
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
        _ => vec![MockCommitItem {
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
        }],
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
    format!(
        r#"--- a/{}
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
 }}"#,
        file_name, file_name
    )
}

#[allow(dead_code)]
fn mock_files_for_commit(sha: &str) -> Vec<MockFileItem> {
    match sha {
        "a3f8c12" => vec![
            MockFileItem {
                name: "modules/security/auditd.nix".to_string(),
                add: 18,
                del: 2,
            },
            MockFileItem {
                name: "modules/security/sudo.nix".to_string(),
                add: 8,
                del: 1,
            },
            MockFileItem {
                name: "hosts/atlas-01/configuration.nix".to_string(),
                add: 2,
                del: 1,
            },
        ],
        "f1d9022" => vec![
            MockFileItem {
                name: "pkgs/openssl/default.nix".to_string(),
                add: 10,
                del: 6,
            },
            MockFileItem {
                name: "flake.lock".to_string(),
                add: 2,
                del: 2,
            },
        ],
        "8c4b311" => vec![MockFileItem {
            name: "hosts/atlas-02/monitoring.nix".to_string(),
            add: 14,
            del: 0,
        }],
        _ => vec![MockFileItem {
            name: "README.md".to_string(),
            add: 5,
            del: 0,
        }],
    }
}

// ============================================================================
// Flake table presentation.
// ============================================================================

#[allow(dead_code)]
#[component]
fn FlakeTableNew(
    flakes: Vec<MockFlakeItem>,
    selected_id: Option<i32>,
    is_admin: bool,
    #[props(default)] env_colors: Vec<EnvironmentSummary>,
    /// When true, error rows receive the attention-flash CSS class (one-shot on first visit).
    #[props(default)]
    flash_errors: bool,
    on_select: EventHandler<MockFlakeItem>,
    on_sync: EventHandler<i32>,
    on_edit: EventHandler<i32>,
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
                        th { "Environments" }
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
                            let is_error = flake.status == "error";
                            // Resolve the same way dismiss_attention_item
                            // resolves its local key: prefer the canonical
                            // server occurrence key (a fresh episode id once
                            // the flake recovers and fails again), falling
                            // back to the stable flake id. Never a composite
                            // including last_sync_at, which changes on every
                            // retry of the same unresolved error and would
                            // never match after a dismiss.
                            let flake_id_str = flake.id.to_string();
                            let flake_key = occurrence_id_for_subject("flakes", &flake_id_str)
                                .unwrap_or(flake_id_str);
                            let row_class = attention_row_class(
                                if is_selected { "selected" } else { "" },
                                "flakes",
                                &flake_key,
                                is_error,
                                is_error && flash_errors,
                            );
                            let flake_for_select = flake.clone();
                            let flake_for_keyboard = flake.clone();
                            let flake_id_for_sync = flake.id;
                            let flake_id_for_edit = flake.id;

                            rsx! {
                                tr {
                                    key: "{flake.id}",
                                    id: "flake-opener-{flake.id}",
                                    class: "{row_class}",
                                    style: "cursor: pointer;",
                                    tabindex: "0",
                                    onkeydown: move |event| {
                                        if event.key() == Key::Enter || event.key() == Key::Character(" ".to_string()) {
                                            event.prevent_default();
                                            on_select.call(flake_for_keyboard.clone());
                                        }
                                    },
                                    onclick: move |_| {
                                        if is_error {
                                            dismiss_attention_item(
                                                "flakes",
                                                &flake_id_for_sync.to_string(),
                                                occurrence_id_for_subject("flakes", &flake_id_for_sync.to_string()).as_deref(),
                                            );
                                        }
                                        on_select.call(flake_for_select.clone());
                                    },

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

                                    td {
                                        FlakeEnvBadgesNew { flake: flake.clone(), max: 3, align: "flex-start", env_colors: env_colors.clone() }
                                    }

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
                                                    title: "Edit flake",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_edit.call(flake_id_for_edit);
                                                    },
                                                    // Inline gear icon
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
// Flake card presentation.
// ============================================================================

#[allow(dead_code)]
#[component]
fn FlakeCardsNew(
    flakes: Vec<MockFlakeItem>,
    selected_id: Option<i32>,
    is_admin: bool,
    #[props(default)] env_colors: Vec<EnvironmentSummary>,
    /// When true, error cards receive the attention-flash CSS class (one-shot on first visit).
    #[props(default)]
    flash_errors: bool,
    on_select: EventHandler<MockFlakeItem>,
    on_sync: EventHandler<i32>,
    on_edit: EventHandler<i32>,
) -> Element {
    rsx! {
        // JSX: <div className="cards-grid">
        div { class: "cards-grid",
            for flake in flakes {
                {
                    let is_selected = selected_id == Some(flake.id);
                    let is_error = flake.status == "error";
                    let flake_for_select = flake.clone();
                    let flake_for_keyboard = flake.clone();
                    let flake_id_for_sync = flake.id;
                    let flake_id_for_edit = flake.id;
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
                    // Same resolution as the table view — prefer the
                    // canonical server occurrence key, falling back to the
                    // stable flake id, matching what dismiss_attention_item
                    // resolves for its local key.
                    let flake_id_str = flake.id.to_string();
                    let flake_key = occurrence_id_for_subject("flakes", &flake_id_str)
                        .unwrap_or(flake_id_str);
                    let card_class = attention_row_class(
                        "sys-card compact",
                        "flakes",
                        &flake_key,
                        is_error,
                        is_error && flash_errors,
                    );

                    rsx! {
                        div {
                            key: "{flake.id}",
                            id: "flake-opener-{flake.id}",
                            class: "{card_class}",
                            style: "{border_style}",
                            role: "button",
                            tabindex: "0",
                            onkeydown: move |event| {
                                if event.key() == Key::Enter || event.key() == Key::Character(" ".to_string()) {
                                    event.prevent_default();
                                    on_select.call(flake_for_keyboard.clone());
                                }
                            },
                            onclick: move |_| {
                                if is_error {
                                    dismiss_attention_item(
                                        "flakes",
                                        &flake_id_for_sync.to_string(),
                                        occurrence_id_for_subject("flakes", &flake_id_for_sync.to_string()).as_deref(),
                                    );
                                }
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
                            }

                            div { style: "display: flex; align-items: baseline; gap: 8px; flex-wrap: wrap;",
                                span { style: "font-size: 10px; text-transform: uppercase; letter-spacing: 0.08em; color: var(--cf-text-muted); font-weight: 600; flex-shrink: 0;", "Environments" }
                                FlakeEnvBadgesNew { flake: flake.clone(), max: 6, align: "flex-start", env_colors: env_colors.clone() }
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
                            // Admin-only actions
                            if is_admin {
                                div { style: "display: flex; gap: 6px; align-items: center;",
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
                                    button {
                                        class: "btn-icon focus-ring",
                                        title: "Edit flake",
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            on_edit.call(flake_id_for_edit);
                                        },
                                        svg {
                                            width: "12",
                                            height: "12",
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
fn EnvBadgeNew(env: String, #[props(default)] color_hex: Option<String>) -> Element {
    let label = if env.trim().is_empty() {
        "not persisted"
    } else {
        env.trim()
    };
    if let Some(hex) = color_hex.as_deref().filter(|h| !h.trim().is_empty()) {
        // Use the environment's assigned color as an inline pill style.
        let hex = hex.trim_start_matches('#');
        let style = format!(
            "background: #{hex}22; color: #{hex}; border: 1px solid #{hex}55; \
             border-radius: 4px; padding: 2px 7px; font-size: 11px; font-weight: 600; \
             white-space: nowrap;"
        );
        rsx! { span { style: "{style}", "{label}" } }
    } else {
        let chip_class = match label {
            "production" => "chip-critical",
            "staging" => "chip-warning",
            "dev" | "edge" => "chip-info",
            _ => "chip-unknown",
        };
        rsx! { span { class: "chip {chip_class}", "{label}" } }
    }
}

#[allow(dead_code)]
#[component]
fn FlakeEnvBadgesNew(
    flake: MockFlakeItem,
    max: usize,
    align: &'static str,
    #[props(default)] env_colors: Vec<EnvironmentSummary>,
) -> Element {
    let envs = if flake.environment.trim().is_empty() {
        Vec::new()
    } else {
        flake
            .environment
            .split(',')
            .map(str::trim)
            .filter(|env| !env.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    };
    let shown = envs.iter().take(max).cloned().collect::<Vec<_>>();
    let extra = envs.len().saturating_sub(shown.len());

    rsx! {
        div { style: "display: flex; align-items: center; gap: 4px; flex-wrap: wrap; justify-content: {align};",
            if shown.is_empty() {
                EnvBadgeNew { env: String::new() }
            } else {
                for env in shown {
                    {
                        let color_hex = env_colors.iter()
                            .find(|e| e.name.eq_ignore_ascii_case(&env))
                            .map(|e| e.color_hex.clone());
                        rsx! { EnvBadgeNew { env, color_hex } }
                    }
                }
                if extra > 0 {
                    span { class: "chip chip-unknown", style: "font-size: 10px;", title: "Additional environments are hidden", "+{extra}" }
                }
            }
        }
    }
}

// ============================================================================
// Revision-scoped flake tray.
// ============================================================================

#[derive(Clone, Debug, PartialEq, Eq)]
struct FlakeOutputPollScope {
    flake_id: i32,
    revision: String,
    filter: FlakeSystemFilter,
    pane: FlakePane,
}

fn flake_snapshot_poll_delay_ms(attempt: u32) -> u32 {
    3_000_u32.saturating_mul(1_u32 << attempt.min(2))
}

#[allow(dead_code)]
#[component]
pub(crate) fn FlakeTrayNew(
    flake: MockFlakeItem,
    commits: Vec<MockCommitItem>,
    commits_loading: bool,
    commits_error: Option<String>,
    notice: Option<String>,
    is_admin: bool,
    focus_sha: Option<String>,
    focus_meta: Option<CommitFocusMeta>,
    initial_pane: FlakePane,
    on_edit: EventHandler<i32>,
    on_sync: EventHandler<i32>,
    on_history_rewrite_conflict: EventHandler<(i32, String)>,
    on_navigation_change: EventHandler<(FlakePane, Option<String>, bool)>,
    on_close: EventHandler<()>,
    #[props(default)] on_open_evaluation: Option<EventHandler<NavigationFocus>>,
    #[props(default)] on_open_build: Option<EventHandler<NavigationFocus>>,
    #[props(default)] on_open_systems: Option<EventHandler<NavigationFocus>>,
) -> Element {
    const INITIAL_VISIBLE_COMMITS: usize = 100;
    const LOAD_MORE_STEP: usize = 100;
    const OUTPUT_PAGE_SIZE: usize = 50;

    // Build effective commit list: prepend synthetic stub for focused SHA not yet in list
    let effective_commits: Vec<MockCommitItem> = {
        if let Some(ref sha) = focus_sha {
            let sha_short = sha.chars().take(7).collect::<String>();
            if !commits
                .iter()
                .any(|commit| commit_has_full_sha(commit, sha))
            {
                let meta = focus_meta.clone().unwrap_or_default();
                let mut v = Vec::with_capacity(commits.len() + 1);
                v.push(MockCommitItem {
                    sha: sha_short.clone(),
                    full_hash: sha.clone(),
                    msg: meta.msg.unwrap_or_else(|| "(deployed commit)".to_string()),
                    author: meta.author.unwrap_or_else(|| "—".to_string()),
                    at: meta.at.unwrap_or_else(|| "deployed".to_string()),
                    committed_at: chrono::Utc::now(),
                    files: 0,
                    add: 0,
                    del: 0,
                    eval_status: None,
                    build_status: None,
                    rollout_on: 0,
                    rollout_total: 0,
                });
                v.extend(commits.iter().cloned());
                v
            } else {
                commits.clone()
            }
        } else {
            commits.clone()
        }
    };

    let mut selected_commit = use_signal(|| {
        // A displayed SHA prefix is never an identity. Fall back to the first
        // commit only when the exact full SHA is not present.
        if let Some(ref sha) = focus_sha {
            if let Some(matched) = effective_commits
                .iter()
                .find(|commit| commit_has_full_sha(commit, sha))
            {
                return Some(matched.clone());
            }
        }
        effective_commits.first().cloned()
    });
    if selected_commit.peek().is_none() {
        let replacement = focus_sha
            .as_ref()
            .and_then(|sha| {
                effective_commits
                    .iter()
                    .find(|commit| commit_has_full_sha(commit, sha))
            })
            .cloned()
            .or_else(|| effective_commits.first().cloned());
        if replacement.is_some() {
            selected_commit.set(replacement);
        }
    }
    if let Some(focus_sha) = focus_sha.as_ref() {
        let replacement = effective_commits
            .iter()
            .find(|commit| commit_has_full_sha(commit, focus_sha))
            .cloned()
            .or_else(|| effective_commits.first().cloned());
        if selected_commit.peek().as_ref() != replacement.as_ref() {
            selected_commit.set(replacement.clone());
        }
        let replacement_revision = replacement.map(|commit| commit.full_hash);
        if replacement_revision.as_deref() != Some(focus_sha) {
            on_navigation_change.call((initial_pane, replacement_revision, false));
        }
    }
    let mut unavailable_commit_hashes = use_signal(Vec::<String>::new);
    let active_pane = initial_pane;
    let mut commit_query = use_signal(String::new);
    let mut visible_limit = use_signal(|| INITIAL_VISIBLE_COMMITS);
    let commits_scroll_id = format!("fl-tray-commits-{}", flake.id);
    let query = commit_query.read().trim().to_lowercase();
    let filtered_commits = if query.is_empty() {
        effective_commits.clone()
    } else {
        effective_commits
            .iter()
            .filter(|commit| {
                commit.sha.to_lowercase().contains(&query)
                    || commit.full_hash.to_lowercase().contains(&query)
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
    let selected_hash = selected_commit
        .read()
        .as_ref()
        .map(|commit| commit.full_hash.clone());
    let mut output_result = use_signal(|| None);
    let mut output_revision_identity = use_signal(|| None::<FlakeOutputSnapshotResponse>);
    let mut output_request_sequence = use_signal(|| 0_u64);
    let mut output_loading_more = use_signal(|| None::<FlakePane>);
    let mut output_continuation_error = use_signal(|| None::<(FlakePane, String)>);
    let mut output_reload_nonce = use_signal(|| 0_u64);
    let mut output_poll_identity = use_signal(|| None::<FlakeOutputPollScope>);
    let mut output_poll_scope = use_signal(|| None::<FlakeOutputPollScope>);
    let mut output_poll_attempt = use_signal(|| 0_u32);
    let mut system_filter = use_signal(FlakeSystemFilter::default);
    let mut tracked_active_pane = use_signal(|| active_pane);
    if *tracked_active_pane.peek() != active_pane {
        tracked_active_pane.set(active_pane);
    }
    {
        let mut request_sequence_on_drop = output_request_sequence;
        let mut poll_scope_on_drop = output_poll_scope;
        use_drop(move || {
            let next_sequence = (*request_sequence_on_drop.peek()).saturating_add(1);
            request_sequence_on_drop.set(next_sequence);
            poll_scope_on_drop.set(None);
        });
    }
    use_effect(move || {
        let _revision = selected_commit
            .read()
            .as_ref()
            .map(|commit| commit.full_hash.clone());
        if *system_filter.peek() != FlakeSystemFilter::All {
            system_filter.set(FlakeSystemFilter::All);
        }
    });
    use_effect(move || {
        let _reload = *output_reload_nonce.read();
        let revision = selected_commit
            .read()
            .as_ref()
            .map(|commit| commit.full_hash.clone());
        if output_revision_identity
            .peek()
            .as_ref()
            .is_some_and(|snapshot| Some(snapshot.revision.as_str()) != revision.as_deref())
        {
            output_revision_identity.set(None);
        }
        output_result.set(None);
        output_loading_more.set(None);
        output_continuation_error.set(None);
        let requested_system_filter = system_filter();
        let requested_pane = tracked_active_pane();
        let next_poll_scope = revision.as_ref().map(|revision| FlakeOutputPollScope {
            flake_id: flake.id,
            revision: revision.clone(),
            filter: requested_system_filter,
            pane: requested_pane,
        });
        if output_poll_identity.peek().as_ref() != next_poll_scope.as_ref() {
            output_poll_identity.set(next_poll_scope);
            output_poll_scope.set(None);
            output_poll_attempt.set(0);
        }
        let sequence = *output_request_sequence.peek() + 1;
        output_request_sequence.set(sequence);
        spawn(async move {
            let result = match revision.as_deref() {
                Some(revision) => fetch_flake_revision_outputs(
                    flake.id,
                    revision,
                    requested_system_filter,
                    OUTPUT_PAGE_SIZE,
                    0,
                    None,
                )
                .await
                .and_then(|page| {
                    validate_flake_output_page_zero(page, revision)
                        .map_err(ApiClientError::Deserialize)
                })
                .map(Some),
                None => Ok(None),
            };
            let still_selected = selected_commit
                .peek()
                .as_ref()
                .map(|commit| commit.full_hash.as_str())
                == revision.as_deref();
            if *output_request_sequence.peek() == sequence
                && still_selected
                && system_filter.peek().eq(&requested_system_filter)
            {
                if let Ok(Some(snapshot)) = &result {
                    output_revision_identity.set(Some(snapshot.clone()));
                }
                output_result.set(Some(result));
            }
        });
    });
    let output_result_value = output_result.read().clone();
    let output_snapshot = output_result_value
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .and_then(Clone::clone);
    let revision_bar_snapshot = output_revision_identity.read().clone();
    let output_error = output_result_value
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .map(flake_snapshot_request_error);
    let output_system_alert = output_snapshot.as_ref().is_some_and(|snapshot| {
        snapshot.declared_unmanaged_count > 0
            || snapshot.managed_undeclared_count > 0
            || snapshot.output_collapsed_count > 0
            || snapshot.pinned_revision_count > 0
    });
    let output_input_alert = output_snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.stale_direct_input_count > 0);
    let output_has_more = output_snapshot
        .as_ref()
        .is_some_and(|snapshot| flake_output_pane_has_more(active_pane, snapshot));
    let active_output_continuation_error = output_continuation_error
        .read()
        .as_ref()
        .filter(|(pane, _)| *pane == active_pane)
        .map(|(_, error)| error.clone());
    {
        let poll_revision = selected_hash.clone();
        use_effect(move || {
            let output_result_for_poll = output_result.read();
            let pending_snapshot = output_result_for_poll
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .and_then(Option::as_ref)
                .is_some_and(|snapshot| {
                    matches!(
                        snapshot.lifecycle,
                        SnapshotLifecycle::Queued | SnapshotLifecycle::Running
                    )
                });
            let terminal_result = output_result_for_poll.is_some() && !pending_snapshot;
            if terminal_result {
                output_poll_scope.set(None);
                output_poll_attempt.set(0);
                return;
            }
            if !pending_snapshot {
                return;
            }
            let Some(revision) = poll_revision.clone() else {
                return;
            };
            let scope = FlakeOutputPollScope {
                flake_id: flake.id,
                revision,
                filter: system_filter(),
                pane: tracked_active_pane(),
            };
            if output_poll_scope.peek().as_ref() == Some(&scope) {
                return;
            }
            // CONCURRENCY: One scoped timer may refresh the selected pane. A pane,
            // revision, filter, or drawer change invalidates the timer before fetch.
            output_poll_scope.set(Some(scope.clone()));
            let delay = flake_snapshot_poll_delay_ms(*output_poll_attempt.peek());
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(delay).await;
                if output_poll_scope.peek().as_ref() != Some(&scope)
                    || selected_commit
                        .peek()
                        .as_ref()
                        .is_none_or(|commit| commit.full_hash != scope.revision)
                    || *system_filter.peek() != scope.filter
                    || *tracked_active_pane.peek() != scope.pane
                {
                    return;
                }
                output_poll_scope.set(None);
                let next_attempt = (*output_poll_attempt.peek()).saturating_add(1);
                let next_reload = (*output_reload_nonce.peek()).saturating_add(1);
                output_poll_attempt.set(next_attempt);
                output_reload_nonce.set(next_reload);
            });
        });
    }
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
    let tray_id = format!("flake-tray-{}", flake.id);
    {
        let tray_id = tray_id.clone();
        use_effect(move || focus_element_by_id(&tray_id));
    }
    let load_more_outputs = {
        let requested_pane = active_pane;
        move |(): ()| {
            let Some(current) = output_result
                .peek()
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .and_then(Clone::clone)
            else {
                return;
            };
            if !flake_output_pane_has_more(requested_pane, &current) {
                return;
            }
            let revision = current.revision.clone();
            let next_offset = current
                .pagination
                .offset
                .saturating_add(current.pagination.limit)
                .min(100_000);
            // COMPATIBILITY: The server owns the continuation-token format. Replay
            // the opaque value unchanged and never derive pagination state from it.
            let snapshot_token = current.snapshot_token.clone();
            let requested_filter = system_filter();
            let sequence = (*output_request_sequence.peek()).saturating_add(1);
            output_request_sequence.set(sequence);
            output_loading_more.set(Some(requested_pane));
            output_continuation_error.set(None);
            spawn(async move {
                let result = fetch_flake_revision_outputs(
                    flake.id,
                    &revision,
                    requested_filter,
                    OUTPUT_PAGE_SIZE,
                    next_offset,
                    snapshot_token.as_deref(),
                )
                .await;
                let still_selected = selected_commit
                    .peek()
                    .as_ref()
                    .is_some_and(|commit| commit.full_hash == revision);
                if *output_request_sequence.peek() != sequence || !still_selected {
                    return;
                }
                output_loading_more.set(None);
                match result {
                    Ok(page) => match merge_flake_output_pages(current, page) {
                        Ok(merged) => output_result.set(Some(Ok(Some(merged)))),
                        Err(error) => output_continuation_error.set(Some((requested_pane, error))),
                    },
                    Err(ApiClientError::Status { code: 409, .. }) => {
                        // CONCURRENCY: The server rejected the immutable page-one
                        // token. Restart at page zero and never mix snapshot versions.
                        output_result.set(None);
                        output_continuation_error.set(None);
                        let next_reload = (*output_reload_nonce.peek()).saturating_add(1);
                        output_reload_nonce.set(next_reload);
                    }
                    Err(error) => output_continuation_error
                        .set(Some((requested_pane, flake_snapshot_request_error(&error)))),
                }
            });
        }
    };
    {
        let mut load_more_inputs = load_more_outputs.clone();
        use_effect(move || {
            let inputs_have_more = output_result
                .read()
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .and_then(Option::as_ref)
                .is_some_and(|snapshot| flake_output_pane_has_more(FlakePane::Inputs, snapshot));
            let continuation_failed = output_continuation_error
                .read()
                .as_ref()
                .is_some_and(|(pane, _)| *pane == FlakePane::Inputs);
            let should_continue = tracked_active_pane() == FlakePane::Inputs
                && inputs_have_more
                && output_loading_more() != Some(FlakePane::Inputs)
                && !continuation_failed;
            if should_continue {
                load_more_inputs(());
            }
        });
    }
    let mut load_more_outputs_button = load_more_outputs.clone();
    let mut retry_outputs_button = load_more_outputs;

    rsx! {
        // JSX: <div className="fl-tray-backdrop" onClick={onClose}/>
        div {
            class: "fl-tray-backdrop",
            onclick: move |_| on_close.call(())
        }

        // JSX: <aside className="fl-tray" role="dialog" aria-label={...}>
        aside {
            class: "fl-tray",
            id: "{tray_id}",
            role: "dialog",
            "aria-modal": "true",
            "aria-label": "{flake.name} commits",
            tabindex: "0",
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    evt.prevent_default();
                    on_close.call(());
                } else {
                    trap_dialog_focus(&evt, &tray_id);
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

            div {
                class: "fx-tabs",
                role: "tablist",
                "aria-label": "Flake revision data",
                onkeydown: move |event| {
                    let panes = [FlakePane::Commits, FlakePane::Systems, FlakePane::Modules, FlakePane::Inputs];
                    let current = panes.iter().position(|pane| pane == &active_pane).unwrap_or_default();
                    let next = match event.key() {
                        Key::ArrowRight => (current + 1) % panes.len(),
                        Key::ArrowLeft => (current + panes.len() - 1) % panes.len(),
                        Key::Home => 0,
                        Key::End => panes.len() - 1,
                        _ => return,
                    };
                    event.prevent_default();
                    on_navigation_change.call((panes[next], selected_commit.read().as_ref().map(|commit| commit.full_hash.clone()), true));
                    focus_element_by_id(&format!("flake-pane-tab-{next}"));
                },
                for (pane_index, pane) in [FlakePane::Commits, FlakePane::Systems, FlakePane::Modules, FlakePane::Inputs].into_iter().enumerate() {
                    button {
                        class: if active_pane == pane { "fx-tab active focus-ring" } else { "fx-tab focus-ring" },
                        role: "tab",
                        "aria-selected": active_pane == pane,
                        "aria-controls": "flake-pane-content",
                        id: "flake-pane-tab-{pane_index}",
                        tabindex: if active_pane == pane { "0" } else { "-1" },
                        onclick: move |_| {
                            on_navigation_change.call((pane, selected_commit.read().as_ref().map(|commit| commit.full_hash.clone()), true));
                        },
                        span { match pane {
                            FlakePane::Commits => "Commits",
                            FlakePane::Systems => "Systems",
                            FlakePane::Modules => "Modules",
                            FlakePane::Inputs => "Inputs",
                        } }
                        span { class: "fx-tab-n", match pane {
                            FlakePane::Commits => effective_commits.len().to_string(),
                            FlakePane::Systems => output_snapshot.as_ref().map(|snapshot| snapshot.declared_system_count).unwrap_or_default().to_string(),
                            FlakePane::Modules => output_snapshot.as_ref().map(|snapshot| snapshot.exported_module_count).unwrap_or_default().to_string(),
                            FlakePane::Inputs => authoritative_input_count(output_snapshot.as_ref()).to_string(),
                        } }
                        if pane == FlakePane::Systems && output_system_alert { span { class: "fx-tab-dot", title: "System reconciliation needs attention" } }
                        if pane == FlakePane::Inputs && output_input_alert { span { class: "fx-tab-dot", title: "One or more direct inputs are stale" } }
                    }
                }
            }

            if active_pane != FlakePane::Commits {
                div { class: "fx-body", id: "flake-pane-content", role: "tabpanel",
                    if output_result_value.is_none() {
                        div { class: "fx-pane",
                            FlakeRevisionBar { commit: active_selected_commit.clone(), snapshot: revision_bar_snapshot.clone() }
                            div { class: "empty", role: "status", "Loading revision outputs…" }
                        }
                    } else if let Some(error) = output_error {
                        div { class: "fx-pane",
                            FlakeRevisionBar { commit: active_selected_commit.clone(), snapshot: revision_bar_snapshot.clone() }
                            div { class: "empty", role: "alert", "Unable to load revision outputs: {error}" }
                        }
                    } else if let Some(snapshot) = output_snapshot.clone() {
                        FlakeOutputPane {
                            pane: active_pane,
                            flake: flake.clone(),
                            commit: selected_commit.read().clone(),
                            snapshot,
                            system_filter: system_filter(),
                            on_system_filter: move |filter| system_filter.set(filter),
                            on_pick_commit: move |_| on_navigation_change.call((
                                FlakePane::Commits,
                                selected_commit.read().as_ref().map(|commit| commit.full_hash.clone()),
                                true,
                            )),
                        }
                        if output_has_more && active_output_continuation_error.is_none() {
                            button {
                                class: "btn btn-ghost focus-ring fx-load-more",
                                disabled: output_loading_more() == Some(active_pane),
                                onclick: move |_| load_more_outputs_button(()),
                                if output_loading_more() == Some(active_pane) { "Loading more…" } else { "Load more revision data" }
                            }
                        }
                        if let Some(error) = active_output_continuation_error {
                            div { class: "cfg-continuation-error", role: "alert",
                                span { "Unable to continue loading this pane: {error}" }
                                button { class: "btn btn-ghost focus-ring xs", disabled: output_loading_more() == Some(active_pane), onclick: move |_| retry_outputs_button(()), "Retry same page" }
                            }
                        }
                    } else {
                        div { class: "empty", style: "margin:18px;", role: "status", "No revision is selected." }
                    }
                }
            }

            if active_pane == FlakePane::Commits {
                if let Some(snapshot) = output_snapshot.clone() {
                    FlakeRevisionBar { commit: active_selected_commit.clone(), snapshot: Some(snapshot) }
                } else {
                    FlakeRevisionBar { commit: active_selected_commit.clone(), snapshot: revision_bar_snapshot.clone() }
                }
            }

            if let Some(msg) = notice {
                div {
                    style: "margin: 0 12px 10px; padding: 8px 10px; border: 1px solid var(--cf-divider); border-radius: 8px; font-size: 12px; color: var(--cf-text-secondary); background: color-mix(in oklab, var(--cf-page-bg) 35%, var(--cf-card-bg));",
                    "{msg}"
                }
            }

            // Sync-error banner — shows when sync_status is "error" (TASK-385)
            if flake.status == "error" {
                if let Some(ref error_msg) = flake.error_msg {
                    {
                        let error_msg = error_msg.clone();
                        let repo_url = flake.url.clone();
                        let latest_commit = if flake.latest_commit == "—" { None } else { Some(flake.latest_commit.clone()) };
                        let flake_id = flake.id;
                        rsx! {
                            FlakeSyncErrorBanner {
                                repo_url,
                                last_sync_error: error_msg,
                                last_sync_at: flake.last_sync_at_raw,
                                latest_commit,
                                on_retry: move |_| on_sync.call(flake_id),
                            }
                        }
                    }
                }
            }

            // Body: Two-pane layout - JSX lines 136-192 (commit list)
            div { class: "fl-tray-body", style: if active_pane == FlakePane::Commits { "display:grid;" } else { "display:none;" },
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
                            on_select: move |commit: MockCommitItem| {
                                on_navigation_change.call((active_pane, Some(commit.full_hash.clone()), true));
                                selected_commit.set(Some(commit));
                            }
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
                                        on_navigation_change.call((
                                            active_pane,
                                            replacement.as_ref().map(|commit| commit.full_hash.clone()),
                                            false,
                                        ));
                                        selected_commit.set(replacement);
                                    },
                                    on_history_rewrite_conflict: on_history_rewrite_conflict,
                                    on_open_evaluation: on_open_evaluation.clone(),
                                    on_open_build: on_open_build.clone(),
                                    on_open_systems: on_open_systems.clone(),
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

#[component]
fn FlakeOutputPane(
    pane: FlakePane,
    flake: MockFlakeItem,
    commit: Option<MockCommitItem>,
    snapshot: FlakeOutputSnapshotResponse,
    system_filter: FlakeSystemFilter,
    on_system_filter: EventHandler<FlakeSystemFilter>,
    on_pick_commit: EventHandler<()>,
) -> Element {
    if snapshot.lifecycle != SnapshotLifecycle::Available {
        let label = match snapshot.lifecycle {
            SnapshotLifecycle::Queued => "Output evaluation is queued.",
            SnapshotLifecycle::Running => "Output evaluation is running.",
            SnapshotLifecycle::Failed => snapshot
                .error
                .as_deref()
                .unwrap_or("Output evaluation failed."),
            SnapshotLifecycle::Unavailable => "No cached output snapshot exists for this revision.",
            SnapshotLifecycle::Available => "",
        };
        let role = if snapshot.lifecycle == SnapshotLifecycle::Failed {
            "alert"
        } else {
            "status"
        };
        return rsx! { div { class: "fx-pane",
            FlakeRevisionBar { commit, snapshot: Some(snapshot.clone()), on_pick_commit: Some(on_pick_commit) }
            div { class: "cfg-state cfg-state-{snapshot_lifecycle_class(snapshot.lifecycle)}", role, strong { "{snapshot_lifecycle_heading(snapshot.lifecycle)}" } p { "{label}" } }
        } };
    }

    rsx! {
        div { class: "fx-pane",
            FlakeRevisionBar { commit, snapshot: Some(snapshot.clone()), on_pick_commit: Some(on_pick_commit) }
            match pane {
                FlakePane::Systems => rsx! { FlakeSystemsOutput { flake, snapshot, system_filter, on_system_filter } },
                FlakePane::Modules => rsx! { FlakeModulesOutput { key: "{snapshot.revision}", flake_id: flake.id, snapshot } },
                FlakePane::Inputs => rsx! { FlakeInputsOutput { snapshot } },
                FlakePane::Commits => rsx! {},
            }
        }
    }
}

#[component]
fn FlakeRevisionBar(
    commit: Option<MockCommitItem>,
    snapshot: Option<FlakeOutputSnapshotResponse>,
    #[props(default)] on_pick_commit: Option<EventHandler<()>>,
) -> Element {
    let revision = snapshot
        .as_ref()
        .map(|snapshot| snapshot.revision.as_str())
        .or_else(|| commit.as_ref().map(|commit| commit.full_hash.as_str()))
        .unwrap_or("revision unavailable");
    let delta = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.delta.as_ref());
    let parent_label = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.first_parent_revision.as_deref())
        .map(short_sha);
    let first_parent_resolved = snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.first_parent_resolved);
    let comparison_available = snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.comparison_available);
    let parent_label_text = parent_label.clone().unwrap_or_else(|| "previous".into());
    rsx! {
        div { class: "fx-revbar",
            div { class: "fx-revbar-main",
                span { class: "fx-revbar-label", "Outputs at" }
                if let Some(on_pick_commit) = on_pick_commit {
                    button {
                        class: "fx-revbar-sha focus-ring",
                        title: "Change commit {revision}",
                        "aria-label": "Change commit. Selected full revision {revision}",
                        onclick: move |_| on_pick_commit.call(()),
                        code { class: "fx-revbar-full", title: "{revision}", "{revision}" }
                        Icon { name: IconName::ChevronRight, size: 11 }
                    }
                } else {
                    code {
                        class: "fx-revbar-full",
                        title: "{revision}",
                        "aria-label": "Full revision {revision}",
                        "{revision}"
                    }
                }
                if let Some(commit) = commit { span { class: "fx-revbar-msg", "{commit.msg}" } }
            }
            div { class: "fx-delta",
                if comparison_available {
                    span { class: "fx-delta-label", "vs {parent_label_text}" }
                    if let Some(delta) = delta {
                        if delta.systems_added_total > 0 { span { class: "fx-delta-chip add", title: delta_sample_title(&delta.systems_added, delta.systems_added_total), "+{delta.systems_added_total} systems" } }
                        if delta.systems_removed_total > 0 { span { class: "fx-delta-chip del", title: delta_sample_title(&delta.systems_removed, delta.systems_removed_total), "-{delta.systems_removed_total} systems" } }
                        if delta.modules_added_total > 0 { span { class: "fx-delta-chip add", title: delta_sample_title(&delta.modules_added, delta.modules_added_total), "+{delta.modules_added_total} modules" } }
                        if delta.modules_removed_total > 0 { span { class: "fx-delta-chip del", title: delta_sample_title(&delta.modules_removed, delta.modules_removed_total), "-{delta.modules_removed_total} modules" } }
                        if delta.inputs_added_total > 0 { span { class: "fx-delta-chip add", title: delta_sample_title(&delta.inputs_added, delta.inputs_added_total), "+{delta.inputs_added_total} inputs" } }
                        if delta.inputs_removed_total > 0 { span { class: "fx-delta-chip del", title: delta_sample_title(&delta.inputs_removed, delta.inputs_removed_total), "-{delta.inputs_removed_total} inputs" } }
                        if delta.input_revision_bumps_total > 0 { span { class: "fx-delta-chip bump", title: input_bump_sample_title(&delta.input_revision_bumps, delta.input_revision_bumps_total), "{delta.input_revision_bumps_total} inputs changed" } }
                        if flake_delta_total(delta) == 0 { span { class: "fx-delta-label", "no output changes" } }
                    } else {
                        span { class: "fx-delta-label", "Comparison details unavailable" }
                    }
                } else if let Some(parent_label) = parent_label {
                    span { class: "fx-delta-label", "vs {parent_label} · comparison snapshot unavailable" }
                } else if first_parent_resolved {
                    span { class: "fx-delta-label", "Root commit · no previous revision" }
                } else {
                    span { class: "fx-delta-label", "First parent unresolved" }
                }
            }
        }
    }
}

#[component]
fn FlakeSystemsOutput(
    flake: MockFlakeItem,
    snapshot: FlakeOutputSnapshotResponse,
    system_filter: FlakeSystemFilter,
    on_system_filter: EventHandler<FlakeSystemFilter>,
) -> Element {
    let unmanaged = snapshot.declared_unmanaged_count;
    let undeclared = snapshot.managed_undeclared_count;
    let all_count = authoritative_system_reconciliation_count(&snapshot);
    let output_collapse = snapshot.output_collapsed_count;
    let pinned = snapshot.pinned_revision_count;
    let declared_output_collapse = declared_output_collapse(
        snapshot.previous_declared_system_count,
        snapshot.declared_system_count,
    );
    rsx! {
        if let Some(previous) = declared_output_collapse {
            div { class: "sd-callout sd-callout-warn fx-callout", Icon { name: IconName::Warn, size: 13 }
                "This revision declares {snapshot.declared_system_count} system outputs; its first parent declared {previous}. Verify the reduction was intentional before deploying."
            }
        }
        if output_collapse > 0 {
            div { class: "sd-callout sd-callout-warn fx-callout", Icon { name: IconName::Warn, size: 13 } "{output_collapse} declared output name(s) are shared by multiple visible managed systems. Review the collapsed mapping before deployment." }
        }
        div { class: "fx-stats",
            FlakeStat { value: snapshot.declared_system_count, label: "declared here" }
            FlakeStat { value: snapshot.managed_system_count, label: "managed by Forge" }
            FlakeStat { value: unmanaged, label: "declared unmanaged", warning: unmanaged > 0 }
            FlakeStat { value: undeclared, label: "managed undeclared", critical: undeclared > 0 }
        }
        if undeclared > 0 {
            div { class: "sd-callout sd-callout-warn fx-callout", "{undeclared} managed system(s) are absent from this revision and remain pinned to an older declared output." }
        }
        if pinned > 0 {
            div { class: "sd-callout sd-callout-warn fx-callout", "{pinned} managed system(s) are deployed at a different revision." }
        }
        div { class: "fx-toolbar",
            div { class: "seg", "aria-label": "Filter reconciled systems",
                for (filter, label, count) in [
                    (FlakeSystemFilter::All, "All", all_count),
                    (FlakeSystemFilter::DeclaredUnmanaged, "Unmanaged", unmanaged),
                    (FlakeSystemFilter::ManagedUndeclared, "Undeclared", undeclared),
                ] {
                    button {
                        class: if system_filter == filter { "active focus-ring" } else { "focus-ring" },
                        onclick: move |_| on_system_filter.call(filter),
                        "{label} " span { class: "seg-n", "{count}" }
                    }
                }
            }
        }
        table { class: "sys-table compact fx-table fx-systems-table",
            colgroup { col { style: "width:34%" } col { style: "width:20%" } col { style: "width:24%" } col { style: "width:22%" } }
            thead { tr { th { "nixosConfiguration" } th { "Environment" } th { "State" } th {} } }
            tbody {
                for system in &snapshot.systems {
                    FlakeSystemRow { key: "{system.configuration_name}-{system.system_id:?}", flake: flake.clone(), revision: snapshot.revision.clone(), system: system.clone() }
                }
                if snapshot.systems.is_empty() { tr { td { colspan: "4", div { class: "fx-empty", "Nothing in this category." } } } }
            }
        }
        div { class: "fx-pane-note", "Showing {snapshot.systems.len()} reconciled rows. Authoritative managed total: {snapshot.managed_system_count}." }
    }
}

#[component]
fn FlakeStat(
    value: i64,
    label: &'static str,
    #[props(default)] warning: bool,
    #[props(default)] critical: bool,
) -> Element {
    let class = if critical {
        "fx-stat crit"
    } else if warning {
        "fx-stat warn"
    } else {
        "fx-stat"
    };
    rsx! { div { class, span { class: "fx-stat-n", "{value}" } span { class: "fx-stat-l", "{label}" } } }
}

#[component]
fn FlakeSystemRow(
    flake: MockFlakeItem,
    revision: String,
    system: ReconciledFlakeSystem,
) -> Element {
    let state_label = match system.state {
        ReconciledFlakeSystemState::Managed => "managed",
        ReconciledFlakeSystemState::DeclaredUnmanaged => "not managed",
        ReconciledFlakeSystemState::ManagedUndeclared => "undeclared",
    };
    let state_class = match system.state {
        ReconciledFlakeSystemState::Managed => "chip chip-healthy fx-chip",
        ReconciledFlakeSystemState::DeclaredUnmanaged => "chip chip-warning fx-chip",
        ReconciledFlakeSystemState::ManagedUndeclared => "chip chip-critical fx-chip",
    };
    let hostname_label = system.hostname.as_deref().unwrap_or("-").to_string();
    let environment_label = system
        .environment_name
        .as_deref()
        .unwrap_or("-")
        .to_string();
    let environment_style = system.environment_color.as_deref().map(|color| {
        format!(
            "color:{color};border-color:color-mix(in oklab, {color} 45%, var(--cf-card-border));"
        )
    });
    rsx! { tr {
        td {
            code { class: "fx-host", title: "{hostname_label}", "{system.configuration_name}" }
            if system.hostname.as_deref().is_some_and(|hostname| hostname != system.configuration_name) {
                span { class: "fx-note", "{hostname_label}" }
            }
            if system.environment_name.is_some() {
                span { class: "fx-system-env-narrow", style: environment_style.clone(), "{environment_label}" }
            }
        }
        td {
            if system.environment_name.is_some() {
                span {
                    class: "chip chip-unknown fx-chip",
                    style: environment_style,
                    "aria-label": "Environment {environment_label}",
                    "{environment_label}"
                }
            } else {
                span { class: "fx-dim", "-" }
            }
        }
        td { span { class: "{state_class}", "{state_label}" }
            if let Some(deployed) = &system.deployed_revision { span { class: "fx-note mono", "pinned at {short_sha(deployed)}" } }
        }
        td { class: "fx-right",
            if let Some(system_id) = system.system_id {
                button { class: "btn btn-ghost focus-ring xs", onclick: move |_| navigate_href(&format!("/systems/{system_id}?tab=config&config_mode=commit&revision={revision}")), "Open config" }
            } else {
                {
                    let href = registration_prefill_url(&system.configuration_name, &flake.name, &flake.branch);
                    rsx! { button { class: "btn btn-ghost focus-ring xs", onclick: move |_| navigate_href(&href), "Add to Forge" } }
                }
            }
        }
    } }
}

#[component]
fn FlakeModulesOutput(flake_id: i32, snapshot: FlakeOutputSnapshotResponse) -> Element {
    const DECLARATION_PAGE_SIZE: usize = 100;

    let mut open = use_signal(|| None::<String>);
    let mut declaration_page = use_signal(|| None::<FlakeModuleDeclarationsPage>);
    let mut declaration_error = use_signal(|| None::<String>);
    let mut declaration_loading = use_signal(|| false);
    let mut declaration_request_sequence = use_signal(|| 0_u64);
    let revision = snapshot.revision.clone();
    let load_declarations = move |module_name: String, append: bool| {
        let current = append.then(|| declaration_page.peek().clone()).flatten();
        let offset = current
            .as_ref()
            .map(|page| page.offset + page.declarations.len())
            .unwrap_or(0);
        let token = current
            .as_ref()
            .and_then(|page| page.snapshot_token.clone());
        // COMPATIBILITY: Declaration continuation tokens are opaque server values.
        if !append {
            declaration_page.set(None);
        }
        declaration_error.set(None);
        declaration_loading.set(true);
        let sequence = *declaration_request_sequence.peek() + 1;
        declaration_request_sequence.set(sequence);
        let request_revision = revision.clone();
        spawn(async move {
            let result = fetch_flake_module_declarations(
                flake_id,
                &request_revision,
                &module_name,
                DECLARATION_PAGE_SIZE,
                offset,
                token.as_deref(),
            )
            .await;
            let still_selected = open.peek().as_deref() == Some(module_name.as_str());
            if *declaration_request_sequence.peek() != sequence || !still_selected {
                return;
            }
            declaration_loading.set(false);
            match result {
                Ok(page) if page.lifecycle == SnapshotLifecycle::Available => {
                    if let Some(current) = current {
                        match merge_module_declaration_pages(current, page) {
                            Ok(merged) => declaration_page.set(Some(merged)),
                            Err(error) => declaration_error.set(Some(error)),
                        }
                    } else {
                        match validate_module_declaration_page_zero(
                            page,
                            &request_revision,
                            &module_name,
                        ) {
                            Ok(page) => declaration_page.set(Some(page)),
                            Err(error) => {
                                declaration_page.set(None);
                                declaration_error.set(Some(error));
                            }
                        }
                    }
                }
                Ok(page) => declaration_error.set(Some(page.error.unwrap_or_else(|| {
                    format!("Declaration snapshot is {:?}.", page.lifecycle).to_lowercase()
                }))),
                Err(error) => {
                    if matches!(error, ApiClientError::Status { code: 409, .. }) {
                        declaration_page.set(None);
                    }
                    declaration_error.set(Some(flake_snapshot_request_error(&error)));
                }
            }
        });
    };
    let modules = flake_output_modules(&snapshot);
    let max_consumers = modules
        .iter()
        .map(|module| module.consumer_count)
        .max()
        .unwrap_or(1)
        .max(1);
    let module_evaluation_error = snapshot
        .outputs
        .as_ref()
        .filter(|outputs| !outputs.module_evaluation.available)
        .and_then(|outputs| outputs.module_evaluation.error.clone())
        .unwrap_or_else(|| "Exported module evaluation is unavailable.".into());
    rsx! {
        if let Some(outputs) = snapshot.outputs.as_ref() {
            if !outputs.module_evaluation.available {
                div { class: "sd-callout sd-callout-warn fx-callout", role: "alert", "{module_evaluation_error}" }
            }
        }
        div { class: "fx-pane-note",
            "Modules exported as " code { "nixosModules" } " at this revision, ordered by authoritative consumer count to show the blast radius of a change. Expand a module to inspect its declarations."
        }
        table { class: "sys-table compact fx-table fx-modules-table",
            colgroup { col { style: "width:32%" } col { style: "width:34%" } col { style: "width:10%" } col { style: "width:24%" } }
            thead { tr { th { "Module" } th { "Sets" } th { class: "fx-right", "Options" } th { "Consumed by" } } }
            tbody {
                for (module_index, module) in modules.iter().enumerate() {
                    {
                        let is_open = open.read().as_deref() == Some(module.name.as_str());
                        let module_for_click = module.name.clone();
                        let detail_id = format!("module-detail-{module_index}");
                        let mut load_for_click = load_declarations.clone();
                        let description = module.description.as_deref().unwrap_or("No description");
                        let consumers = module.consumers.join(", ");
                        let module_source = module_binding_label(module);
                        rsx! {
                            tr {
                                class: "fx-row",
                                td { button {
                                    class: "fx-row-toggle focus-ring",
                                    "aria-expanded": is_open,
                                    "aria-controls": "{detail_id}",
                                    onclick: move |_| if is_open {
                                        let next_sequence = *declaration_request_sequence.peek() + 1;
                                        declaration_request_sequence.set(next_sequence);
                                        declaration_loading.set(false);
                                        declaration_page.set(None);
                                        declaration_error.set(None);
                                        open.set(None);
                                    } else {
                                        open.set(Some(module_for_click.clone()));
                                        load_for_click(module_for_click.clone(), false);
                                    },
                                    div { class: "fx-mod-cell",
                                        span { class: if is_open { "cfg-caret open" } else { "cfg-caret" }, Icon { name: IconName::ChevronRight, size: 11 } }
                                        code { class: "fx-host", title: module.source_path.clone().unwrap_or_default(), "{module.name}" }
                                    }
                                } }
                                td {
                                    span { class: "fx-desc", "{description}" }
                                    span {
                                        class: "fx-note mono",
                                        title: module.source_revision.clone().unwrap_or_default(),
                                        "{module_source}"
                                    }
                                    if let Some(error) = &module.error { span { class: "fx-note", "{error}" } }
                                }
                                td { class: "fx-right", span { class: "mono fx-dim", "{module.declaration_count}" } }
                                td {
                                    div { class: "fx-bar-row",
                                        div { class: "fx-bar", "aria-hidden": "true",
                                            div {
                                                class: "fx-bar-fill",
                                                style: "width: {module.consumer_count * 100 / max_consumers}%",
                                            }
                                        }
                                        span { class: "mono fx-bar-n", "{module.consumer_count}" }
                                    }
                                    if !module.consumers.is_empty() { span { class: "fx-note", "{consumers}" } }
                                }
                            }
                            if is_open { tr { class: "fx-detail-row", id: "{detail_id}", td { colspan: "4", div { class: "fx-detail",
                                div { class: "fx-detail-head",
                                    span { class: "fx-detail-label", "Declared options" }
                                    span {
                                        class: "mono fx-detail-file",
                                        title: module.source_revision.clone().unwrap_or_default(),
                                        "{module_source}"
                                    }
                                }
                                if declaration_loading() && declaration_page.read().is_none() {
                                    div { class: "fx-dim", role: "status", "Loading declarations…" }
                                }
                                if let Some(page) = declaration_page.read().as_ref() {
                                table { class: "fx-opts",
                                    thead { tr { th { scope: "col", "Option" } th { scope: "col", "Type" } th { scope: "col", "Default" } } }
                                    tbody {
                                        for declaration in &page.declarations {
                                            {
                                                let default_label = declaration.default.as_ref().map(render_json_compact).unwrap_or_else(|| "null".into());
                                                let source_paths = declaration.source_paths.join(", ");
                                                rsx! { tr {
                                                    td {
                                                        code { class: "fx-opt-path", "{declaration.path}" }
                                                        if !declaration.source_paths.is_empty() {
                                                            span { class: "fx-declaration-source mono", "Source: {source_paths}" }
                                                        }
                                                    }
                                                    td { span { class: "fx-dim", "{declaration.declared_type}" } }
                                                    td { code { class: "fx-dim", if declaration.has_default { "{default_label}" } else { "No default" } } }
                                                } }
                                            }
                                        }
                                    }
                                }
                                if page.declarations.is_empty() { span { class: "fx-dim", "No declarations are exported by this module." } }
                                if page.declarations.len() < usize::try_from(page.total).unwrap_or(usize::MAX) {
                                    {
                                        let module_name = module.name.clone();
                                        let mut load_more = load_declarations.clone();
                                        rsx! { button {
                                            class: "btn btn-ghost focus-ring fx-load-more",
                                            "aria-label": "Load more declarations for {module.name}",
                                            disabled: declaration_loading(),
                                            onclick: move |_| load_more(module_name.clone(), true),
                                            if declaration_loading() { "Loading more declarations…" } else { "Load more declarations" }
                                        } }
                                    }
                                }
                                div { class: "fx-detail-note", "Declarations come from the evaluation already run for builds, cached per revision; browsing does not evaluate each host." }
                                }
                                if let Some(error) = declaration_error.read().as_ref() {
                                    div { class: "sd-callout sd-callout-warn fx-callout", role: "alert", "Unable to load declarations: {error}" }
                                    {
                                        let module_name = module.name.clone();
                                        let append = declaration_page.read().is_some();
                                        let mut retry = load_declarations.clone();
                                        rsx! { button {
                                            class: "btn btn-ghost focus-ring",
                                            disabled: declaration_loading(),
                                            onclick: move |_| retry(module_name.clone(), append),
                                            "Retry declarations"
                                        } }
                                    }
                                }
                            } } } }
                        }
                    }
                }
                if modules.is_empty() { tr { td { colspan: "4", div { class: "fx-empty", "No exported nixosModules at this revision." } } } }
            }
        }
    }
}

#[component]
fn FlakeInputsOutput(snapshot: FlakeOutputSnapshotResponse) -> Element {
    let inputs = flake_output_inputs(&snapshot);
    let stale = snapshot.stale_direct_input_count;
    let outputs = snapshot.outputs.as_ref();
    let direct_count = outputs
        .map(|outputs| outputs.direct_input_count)
        .unwrap_or_default();
    let resolved_count = outputs
        .map(|outputs| outputs.resolved_input_count)
        .unwrap_or_default();
    let nixpkgs_revision_count = outputs
        .map(|outputs| outputs.nixpkgs_revisions.len())
        .unwrap_or_default();
    rsx! {
        div { class: "fx-stats",
            FlakeStat { value: direct_count, label: "direct inputs" }
            FlakeStat { value: resolved_count, label: "resolved total" }
            FlakeStat { value: nixpkgs_revision_count as i64, label: "nixpkgs revisions", warning: outputs.is_some_and(|outputs| outputs.multiple_nixpkgs_revisions) }
            FlakeStat { value: stale, label: "stale over 90d", warning: stale > 0 }
        }
        if outputs.is_some_and(|outputs| outputs.multiple_nixpkgs_revisions) { div { class: "sd-callout sd-callout-info fx-callout", "This snapshot resolves multiple nixpkgs revisions. Hosts may not share one package set." } }
        if let Some(error) = outputs.and_then(|outputs| outputs.lock_error.as_deref()) { div { class: "sd-callout sd-callout-warn fx-callout", "{error}" } }
        table { class: "sys-table compact fx-table fx-inputs-table",
            colgroup { col { style: "width:27%" } col { style: "width:31%" } col { style: "width:12%" } col { style: "width:14%" } col { style: "width:16%" } }
            thead { tr { th { "Input" } th { "Source" } th { "Locked" } th { "Updated" } th { "Follows" } } }
            tbody {
                for input in &inputs {
                    {
                        let age = input_age_days(input.last_modified);
                        let age_label = age.map(|days| format!("{days}d ago")).unwrap_or_else(|| "unknown".into());
                        let source_label = input.source.as_deref().unwrap_or("unavailable").to_string();
                        let revision_label = input.locked_revision.as_deref().map(short_sha).unwrap_or_else(|| "unavailable".into());
                        let aliases = input.names.join(", ");
                        let follows = input.follows.iter().map(render_follows_value).collect::<Vec<_>>().join(", ");
                        let revision_bump = snapshot.delta.as_ref().and_then(|delta| delta.input_revision_bumps.iter().find(|bump| bump.node == input.node));
                        let bump_before = revision_bump.and_then(|bump| bump.before.as_deref()).map(short_sha).unwrap_or_else(|| "none".into());
                        let bump_after = revision_bump.and_then(|bump| bump.after.as_deref()).map(short_sha).unwrap_or_else(|| "none".into());
                        let original = render_json_compact(&input.original);
                        let locked = render_json_compact(&input.locked);
                        rsx! { tr {
                            td {
                                div { class: "fx-input-cell",
                                    code { class: "fx-host", title: "{input.node}", "{input.names.first().unwrap_or(&input.node)}" }
                                    if input.direct { span { class: "chip chip-info fx-chip fx-chip-shrink", "direct" } }
                                    else if input.transitive { span { class: "chip chip-unknown fx-chip fx-chip-shrink", "transitive" } }
                                    if input.tracked { span { class: "chip chip-info fx-chip fx-chip-shrink", title: "Revision-tracked input", "tracked" } }
                                    if input.channel { span { class: "chip chip-unknown fx-chip fx-chip-shrink", title: "Channel input", "channel" } }
                                }
                                if input.names.len() > 1 { span { class: "fx-note", "aliases: {aliases}" } }
                                div { class: "fx-input-source-narrow",
                                    code { class: "fx-url", title: input.source.clone().unwrap_or_default(), "{source_label}" }
                                    span { class: "fx-note", "{input.source_type}" }
                                    details { class: "fx-input-metadata", summary { "Lock metadata" } code { "original: {original}" } code { "locked: {locked}" } }
                                }
                            }
                            td {
                                code { class: "fx-url", title: input.source.clone().unwrap_or_default(), "{source_label}" }
                                span { class: "fx-note", "{input.source_type}" }
                                details { class: "fx-input-metadata", summary { "Lock metadata" } code { "original: {original}" } code { "locked: {locked}" } }
                            }
                            td {
                                code { class: "fx-dim", title: input.locked_revision.clone().unwrap_or_default(), "{revision_label}" }
                                if revision_bump.is_some() { span { class: "fx-note mono", "{bump_before} -> {bump_after}" } }
                            }
                            td { span { class: if age.is_some_and(|days| days > 90) { "fx-stale" } else { "fx-dim" }, "{age_label}" } }
                            td {
                                if !input.follows.is_empty() { span { class: "mono fx-dim", "{follows}" } } else { span { class: "fx-dim", "-" } }
                                if let Some(descendants) = input.transitive_descendant_count {
                                    span { class: "fx-note", "+{descendants} transitive" }
                                }
                            }
                        } }
                    }
                }
                if inputs.is_empty() { tr { td { colspan: "5", div { class: "fx-empty", "No input metadata is available at this revision." } } } }
            }
        }
    }
}

fn flake_output_modules(snapshot: &FlakeOutputSnapshotResponse) -> Vec<FlakeOutputModule> {
    let mut modules = snapshot
        .outputs
        .as_ref()
        .map(|outputs| outputs.exported_modules.clone())
        .unwrap_or_default();
    modules.sort_by(|left, right| {
        right
            .consumer_count
            .cmp(&left.consumer_count)
            .then_with(|| left.name.cmp(&right.name))
    });
    modules
}

fn flake_output_inputs(snapshot: &FlakeOutputSnapshotResponse) -> Vec<FlakeOutputInput> {
    let mut inputs = snapshot
        .outputs
        .as_ref()
        .map(|outputs| outputs.inputs.clone())
        .unwrap_or_default();
    inputs.sort_by(|left, right| {
        right.direct.cmp(&left.direct).then_with(|| {
            left.names
                .first()
                .unwrap_or(&left.node)
                .cmp(right.names.first().unwrap_or(&right.node))
        })
    });
    inputs
}

fn authoritative_input_count(snapshot: Option<&FlakeOutputSnapshotResponse>) -> i64 {
    snapshot
        .and_then(|snapshot| snapshot.outputs.as_ref())
        .map(|outputs| outputs.direct_input_count)
        .unwrap_or_default()
}

fn authoritative_system_reconciliation_count(snapshot: &FlakeOutputSnapshotResponse) -> i64 {
    snapshot
        .managed_system_count
        .saturating_add(snapshot.declared_unmanaged_count)
}

fn delta_sample_title(items: &[String], total: usize) -> String {
    let mut title = items.join(", ");
    let omitted = total.saturating_sub(items.len());
    if omitted > 0 {
        title.push_str(&format!("\n{omitted} more not shown"));
    }
    title
}

fn input_bump_sample_title(
    bumps: &[crate::api::models::FlakeInputRevisionBump],
    total: usize,
) -> String {
    let mut title = bumps
        .iter()
        .map(|bump| {
            format!(
                "{}: {} -> {}",
                bump.node,
                bump.before
                    .as_deref()
                    .map(short_sha)
                    .unwrap_or_else(|| "none".into()),
                bump.after
                    .as_deref()
                    .map(short_sha)
                    .unwrap_or_else(|| "none".into()),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let omitted = total.saturating_sub(bumps.len());
    if omitted > 0 {
        title.push_str(&format!("\n{omitted} more not shown"));
    }
    title
}

fn flake_delta_total(delta: &FlakeOutputDelta) -> usize {
    delta
        .systems_added_total
        .saturating_add(delta.systems_removed_total)
        .saturating_add(delta.modules_added_total)
        .saturating_add(delta.modules_removed_total)
        .saturating_add(delta.inputs_added_total)
        .saturating_add(delta.inputs_removed_total)
        .saturating_add(delta.input_revision_bumps_total)
}

fn declared_output_collapse(previous: Option<i64>, selected: i64) -> Option<i64> {
    previous.filter(|previous| selected < *previous)
}

fn flake_output_pane_has_more(pane: FlakePane, snapshot: &FlakeOutputSnapshotResponse) -> bool {
    if snapshot.lifecycle != SnapshotLifecycle::Available
        || snapshot.pagination.offset >= 100_000
        || snapshot.pagination.limit == 0
    {
        return false;
    }
    match pane {
        FlakePane::Commits => false,
        FlakePane::Systems => snapshot.pagination.systems_has_more,
        FlakePane::Modules => i64::try_from(flake_output_modules(snapshot).len())
            .is_ok_and(|loaded| loaded < snapshot.exported_module_count),
        FlakePane::Inputs => snapshot.outputs.as_ref().is_some_and(|outputs| {
            i64::try_from(flake_output_inputs(snapshot).len())
                .is_ok_and(|loaded| loaded < outputs.resolved_input_count)
        }),
    }
}

fn validate_flake_output_page_zero(
    page: FlakeOutputSnapshotResponse,
    revision: &str,
) -> Result<FlakeOutputSnapshotResponse, String> {
    if page.revision != revision || page.pagination.offset != 0 {
        return Err("The first revision-output page did not match the selected revision.".into());
    }
    if page.lifecycle == SnapshotLifecycle::Available
        && page
            .snapshot_token
            .as_deref()
            .is_none_or(|token| token.trim().is_empty())
    {
        return Err("The revision-output response did not include a snapshot token.".into());
    }
    Ok(page)
}

fn merge_flake_output_pages(
    mut accumulated: FlakeOutputSnapshotResponse,
    page: FlakeOutputSnapshotResponse,
) -> Result<FlakeOutputSnapshotResponse, String> {
    let expected_offset = accumulated
        .pagination
        .offset
        .saturating_add(accumulated.pagination.limit);
    if accumulated.lifecycle != SnapshotLifecycle::Available
        || page.lifecycle != SnapshotLifecycle::Available
        || accumulated.revision != page.revision
        || accumulated.first_parent_revision != page.first_parent_revision
        || accumulated.first_parent_resolved != page.first_parent_resolved
        || accumulated.comparison_available != page.comparison_available
        || accumulated.snapshot_token.is_none()
        || accumulated.snapshot_token != page.snapshot_token
        || accumulated.managed_system_count != page.managed_system_count
        || accumulated.declared_system_count != page.declared_system_count
        || accumulated.previous_declared_system_count != page.previous_declared_system_count
        || accumulated.declared_unmanaged_count != page.declared_unmanaged_count
        || accumulated.managed_undeclared_count != page.managed_undeclared_count
        || accumulated.output_collapsed_count != page.output_collapsed_count
        || accumulated.pinned_revision_count != page.pinned_revision_count
        || accumulated.stale_direct_input_count != page.stale_direct_input_count
        || accumulated.exported_module_count != page.exported_module_count
        || page.pagination.offset != expected_offset
    {
        return Err("The revision-output snapshot changed. Reload this pane to continue.".into());
    }
    accumulated.outputs = merge_flake_output_payloads(accumulated.outputs, page.outputs);
    accumulated.previous_outputs =
        merge_flake_output_payloads(accumulated.previous_outputs, page.previous_outputs);
    accumulated.systems.extend(page.systems);
    let mut system_keys = HashSet::new();
    accumulated
        .systems
        .retain(|system| system_keys.insert((system.configuration_name.clone(), system.system_id)));
    accumulated.error = page.error;
    accumulated.pagination = page.pagination;
    Ok(accumulated)
}

fn merge_flake_output_payloads(
    accumulated: Option<FlakeOutputPayload>,
    page: Option<FlakeOutputPayload>,
) -> Option<FlakeOutputPayload> {
    match (accumulated, page) {
        (Some(mut accumulated), Some(page)) => {
            accumulated.declared_systems.extend(page.declared_systems);
            accumulated.exported_modules.extend(page.exported_modules);
            accumulated.inputs.extend(page.inputs);
            deduplicate_strings(&mut accumulated.declared_systems);
            let mut module_names = HashSet::new();
            accumulated
                .exported_modules
                .retain(|module| module_names.insert(module.name.clone()));
            let mut input_nodes = HashSet::new();
            accumulated
                .inputs
                .retain(|input| input_nodes.insert(input.node.clone()));
            accumulated.direct_input_count = page.direct_input_count;
            accumulated.resolved_input_count = page.resolved_input_count;
            accumulated.lock_error = page.lock_error;
            accumulated.module_evaluation = page.module_evaluation;
            accumulated.nixpkgs_revisions = page.nixpkgs_revisions;
            accumulated.multiple_nixpkgs_revisions = page.multiple_nixpkgs_revisions;
            Some(accumulated)
        }
        (None, page) => page,
        (accumulated, None) => accumulated,
    }
}

fn merge_flake_output_deltas(
    accumulated: Option<FlakeOutputDelta>,
    page: Option<FlakeOutputDelta>,
) -> Option<FlakeOutputDelta> {
    match (accumulated, page) {
        (Some(mut accumulated), Some(page)) => {
            accumulated.systems_added.extend(page.systems_added);
            accumulated.systems_removed.extend(page.systems_removed);
            accumulated.modules_added.extend(page.modules_added);
            accumulated.modules_removed.extend(page.modules_removed);
            accumulated.inputs_added.extend(page.inputs_added);
            accumulated.inputs_removed.extend(page.inputs_removed);
            accumulated
                .input_revision_bumps
                .extend(page.input_revision_bumps);
            deduplicate_strings(&mut accumulated.systems_added);
            deduplicate_strings(&mut accumulated.systems_removed);
            deduplicate_strings(&mut accumulated.modules_added);
            deduplicate_strings(&mut accumulated.modules_removed);
            deduplicate_strings(&mut accumulated.inputs_added);
            deduplicate_strings(&mut accumulated.inputs_removed);
            let mut bump_nodes = HashSet::new();
            accumulated
                .input_revision_bumps
                .retain(|bump| bump_nodes.insert(bump.node.clone()));
            Some(accumulated)
        }
        (None, page) => page,
        (accumulated, None) => accumulated,
    }
}

fn merge_module_declaration_pages(
    mut accumulated: FlakeModuleDeclarationsPage,
    page: FlakeModuleDeclarationsPage,
) -> Result<FlakeModuleDeclarationsPage, String> {
    let expected_offset = accumulated.offset + accumulated.declarations.len();
    if accumulated.lifecycle != SnapshotLifecycle::Available
        || page.lifecycle != SnapshotLifecycle::Available
        || accumulated.revision != page.revision
        || accumulated.module_name != page.module_name
        || accumulated.snapshot_token.is_none()
        || accumulated.snapshot_token != page.snapshot_token
        || accumulated.total != page.total
        || page.offset != expected_offset
    {
        return Err("The declaration snapshot changed. Reload this module to continue.".into());
    }
    let existing_paths = accumulated
        .declarations
        .iter()
        .map(|declaration| declaration.path.clone())
        .collect::<HashSet<_>>();
    if page
        .declarations
        .iter()
        .any(|declaration| existing_paths.contains(&declaration.path))
    {
        return Err("The declaration page overlaps previously loaded rows.".into());
    }
    accumulated.declarations.extend(page.declarations);
    accumulated.limit = page.limit;
    accumulated.error = page.error;
    Ok(accumulated)
}

fn validate_module_declaration_page_zero(
    page: FlakeModuleDeclarationsPage,
    revision: &str,
    module_name: &str,
) -> Result<FlakeModuleDeclarationsPage, String> {
    let row_count = i64::try_from(page.declarations.len()).unwrap_or(i64::MAX);
    let token_is_valid = page
        .snapshot_token
        .as_deref()
        .is_some_and(|token| !token.trim().is_empty());
    if page.lifecycle != SnapshotLifecycle::Available
        || page.revision != revision
        || page.module_name != module_name
        || page.offset != 0
        || !token_is_valid
        || page.total < 0
        || row_count > page.total
        || page.declarations.len() > page.limit
        || (page.total > 0 && page.declarations.is_empty())
        || (page.total > 0 && page.limit == 0)
    {
        return Err(
            "The first declaration page does not match the selected module snapshot. Retry from the first page."
                .into(),
        );
    }
    Ok(page)
}

fn input_age_days(last_modified: Option<i64>) -> Option<i64> {
    last_modified.map(|timestamp| (Utc::now().timestamp() - timestamp).max(0) / 86_400)
}

fn render_json_compact(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "unavailable".into())
}

fn render_follows_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Array(path) if path.iter().all(serde_json::Value::is_string) => path
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>()
            .join("/"),
        _ => render_json_compact(value),
    }
}

fn module_binding_label(module: &FlakeOutputModule) -> String {
    match (
        module.source_input.as_deref(),
        module.source_path.as_deref(),
    ) {
        (Some(input), Some(path)) => format!("{input} / {path}"),
        (None, Some(path)) => format!("untracked / {path}"),
        (_, None) => "Export binding unavailable".into(),
    }
}

fn deduplicate_strings(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn snapshot_lifecycle_class(lifecycle: SnapshotLifecycle) -> &'static str {
    match lifecycle {
        SnapshotLifecycle::Queued | SnapshotLifecycle::Running => "pending",
        SnapshotLifecycle::Failed => "failed",
        SnapshotLifecycle::Available => "available",
        SnapshotLifecycle::Unavailable => "unavailable",
    }
}

fn snapshot_lifecycle_heading(lifecycle: SnapshotLifecycle) -> &'static str {
    match lifecycle {
        SnapshotLifecycle::Queued => "Revision outputs queued",
        SnapshotLifecycle::Running => "Revision outputs running",
        SnapshotLifecycle::Failed => "Revision outputs failed",
        SnapshotLifecycle::Available => "Revision outputs available",
        SnapshotLifecycle::Unavailable => "Revision outputs unavailable",
    }
}

fn flake_snapshot_request_error(error: &ApiClientError) -> String {
    match error {
        ApiClientError::Status { code: 401, .. } => {
            "Sign in again to view these revision outputs.".into()
        }
        ApiClientError::Status {
            code: 403 | 404, ..
        } => "These revision outputs are unavailable or you do not have access.".into(),
        ApiClientError::Status { code: 409, .. } => {
            "The snapshot changed. Reload declarations from the first page.".into()
        }
        _ => format!("The revision-output request failed: {error}"),
    }
}

fn short_sha(revision: &str) -> String {
    revision.chars().take(7).collect()
}

fn registration_prefill_url(configuration: &str, flake: &str, branch: &str) -> String {
    format!(
        "/systems?add=1&hostname={}&configuration={}&flake_name={}&branch={}",
        encode_query_component(configuration),
        encode_query_component(configuration),
        encode_query_component(flake),
        encode_query_component(branch)
    )
}

fn encode_query_component(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn navigate_href(href: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(href);
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
    on_select: EventHandler<MockCommitItem>,
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
    is_last_bucket: bool,
) -> Element {
    let total_commits = commits.len();

    rsx! {
        div {
            // Bucket header - JSX line 153
            div { class: "fl-commits-bucket", "{bucket_name}" }

            // Commit items - JSX lines 154-183
            for (i, commit) in commits.iter().enumerate() {
                {
                    let is_selected = selected_commit
                        .as_ref()
                        .is_some_and(|selected| selected.full_hash == commit.full_hash);
                    let is_last_in_bucket = i == total_commits - 1;
                    let pipeline_status = MockPipelineStatus {
                        eval: commit.eval_status.clone(),
                        build: commit.build_status.clone(),
                    };

                    rsx! {
                        CommitItemNew {
                            key: "{commit.full_hash}",
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
    on_select: EventHandler<MockCommitItem>,
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

    let dot_class = if is_selected { "fl-dot sel" } else { "fl-dot" };
    let commit_for_select = commit.clone();

    rsx! {
        button {
            r#type: "button",
            class: "{item_class}",
            "aria-pressed": is_selected,
            "aria-label": "Commit {commit.full_hash}: {commit.msg}",
            onclick: move |_| on_select.call(commit_for_select.clone()),

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
                        title: "{commit.full_hash}",
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
    on_open_evaluation: Option<EventHandler<NavigationFocus>>,
    on_open_build: Option<EventHandler<NavigationFocus>>,
    on_open_systems: Option<EventHandler<NavigationFocus>>,
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
    let commit_hash_for_eval = commit.full_hash.clone();
    let commit_hash_for_build = commit.full_hash.clone();
    let flake_name_for_eval = flake.name.clone();
    let flake_name_for_build = flake.name.clone();
    let flake_name_for_systems = flake.name.clone();

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
                PipelinePillNew {
                    stage: "eval",
                    val: pipeline.eval.clone(),
                    onclick: move |_| {
                        if let Some(handler) = &on_open_evaluation {
                            handler.call(NavigationFocus {
                                target: FocusTarget::Evaluations,
                                commit_sha: Some(commit_hash_for_eval.clone()),
                                flake_name: Some(flake_name_for_eval.clone()),
                                status: pipeline.eval.clone(),
                                policy_name: None,
                            });
                        }
                    }
                }
                PipelineArrowNew {}
                PipelinePillNew {
                    stage: "build",
                    val: pipeline.build.clone(),
                    onclick: move |_| {
                        if let Some(handler) = &on_open_build {
                            handler.call(NavigationFocus {
                                target: FocusTarget::Builds,
                                commit_sha: Some(commit_hash_for_build.clone()),
                                flake_name: Some(flake_name_for_build.clone()),
                                status: pipeline.build.clone(),
                                policy_name: None,
                            });
                        }
                    }
                }
                PipelineArrowNew {}
                RolloutPillNew {
                    on: commit.rollout_on,
                    total: commit.rollout_total,
                    failed: 0,
                    onclick: move |_| {
                        if let Some(handler) = &on_open_systems {
                            handler.call(NavigationFocus {
                                target: FocusTarget::Systems,
                                commit_sha: None,
                                flake_name: Some(flake_name_for_systems.clone()),
                                status: None,
                                policy_name: None,
                            });
                        }
                    }
                }
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
                            let file_focus_id = stable_dom_id("flake-file", &file.name);
                            rsx! {
                                FileCardNew {
                                    file: file.clone(),
                                    focus_id: file_focus_id,
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
                {
                    let file_focus_id = stable_dom_id("flake-file", &file.name);
                    rsx! {
                DiffModalNew {
                    file,
                    commit: commit.clone(),
                    flake: flake.clone(),
                    on_close: move |_| {
                        active_modal_file.set(None);
                        focus_element_by_id(&file_focus_id);
                    },
                }
                    }
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
fn PipelinePillNew(
    stage: &'static str,
    val: Option<String>,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
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
        button {
            class: "chip {chip_class} focus-ring",
            style: "font-weight: 600; cursor: pointer;",
            onclick: move |evt| {
                if let Some(handler) = &onclick {
                    handler.call(evt);
                }
            },
            "{label}"
        }
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
fn RolloutPillNew(
    on: i32,
    total: i32,
    failed: i32,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    let pct = if total > 0 {
        (on as f32) / (total as f32)
    } else {
        0.0
    };
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
        button {
            class: "chip {chip_class}",
            style: "display: inline-flex; align-items: center; gap: 6px; font-weight: 600; cursor: pointer;",
            onclick: move |evt| {
                if let Some(handler) = &onclick {
                    handler.call(evt);
                }
            },
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
fn FileCardNew(
    file: MockFileItem,
    focus_id: String,
    is_selected: bool,
    on_select: EventHandler<MockFileItem>,
) -> Element {
    let file_for_click = file.clone();
    let total = (file.add + file.del) as f32 + 0.001;
    let add_pct = ((file.add as f32 / total) * 100.0).round() as i32;
    let del_pct = ((file.del as f32 / total) * 100.0).round() as i32;

    // Split path into filename and directory
    let parts: Vec<&str> = file.name.split('/').collect();
    let filename = parts.last().unwrap_or(&"");
    let directory = if parts.len() > 1 {
        parts[..parts.len() - 1].join("/")
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
            id: "{focus_id}",
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
    line_type: String, // "hunk", "meta", "add", "del", "ctx"
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
    on_close: EventHandler<()>,
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
    let modal_id = stable_dom_id("flake-diff-modal", &file.name);
    {
        let modal_id = modal_id.clone();
        use_effect(move || focus_element_by_id(&modal_id));
    }
    let modal_id_for_keydown = modal_id.clone();

    rsx! {
        // Backdrop - JSX line 331
        div {
            class: "modal-backdrop modal-backdrop-above-drawer",
            onclick: move |_| on_close.call(()),
            tabindex: "0",
            onkeydown: move |evt| {
                evt.stop_propagation();
                if evt.key() == Key::Escape {
                    evt.prevent_default();
                    on_close.call(());
                } else {
                    trap_dialog_focus(&evt, &modal_id_for_keydown);
                }
            },

            // Modal content - JSX line 332
            div {
                class: "diff-modal",
                id: "{modal_id}",
                role: "dialog",
                "aria-modal": "true",
                tabindex: "-1",
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
