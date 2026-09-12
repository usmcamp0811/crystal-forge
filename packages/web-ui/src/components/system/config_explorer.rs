//! Renders lazy, path-scoped system configuration observations.
//!
//! The Explorer is observational and independent from certified Config
//! snapshots. Structured path components remain the request identity. Dotted
//! paths are presentation only.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use dioxus::prelude::*;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::api::client::{ApiClientError, load_system_config_observation};
use crate::api::models::{
    ConfigObservationChild, ConfigObservationChildKind, ConfigObservationKind,
    ConfigObservationLifecycle, ConfigObservationPayload, ConfigObservationResponse,
    ConfiguredOptionIdentity, CreateConfigObservationRequest, EvaluatedOption,
    EvaluatedOptionCounts, EvaluatedOptionFilter, EvaluatedOptionRow, EvaluationModuleSummary,
    OptionDefinitionProvenance, OptionInventoryState, SafeOptionValue, SnapshotLifecycle,
};
use crate::components::icon::{Icon, IconName};

#[derive(Clone, PartialEq)]
enum ObservationState {
    Idle,
    Lifecycle(ConfigObservationLifecycle),
    Loaded(ConfigObservationResponse),
    Error(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExplorerMode {
    Browse,
    Configured,
    Search,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InspectorPane {
    Option,
    Sources,
}

fn should_start_configured(mode: ExplorerMode, state: &ObservationState) -> bool {
    mode == ExplorerMode::Configured && matches!(state, ObservationState::Idle)
}

fn initial_scoped_operations(
    revision: Option<&str>,
    scoped_enabled: bool,
) -> Vec<ConfigObservationKind> {
    if scoped_enabled && revision.is_some() {
        vec![ConfigObservationKind::Root]
    } else {
        Vec::new()
    }
}

fn next_operation_sequence(current: u64) -> u64 {
    current.saturating_add(1)
}

fn should_request_prefix(child_offset: u32, loaded: bool) -> bool {
    child_offset != 0 || !loaded
}

fn display_path_component(component: &str) -> String {
    let mut chars = component.chars();
    let starts_like_identifier = chars
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_');
    if starts_like_identifier
        && chars.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '\'')
        })
    {
        component.to_string()
    } else {
        serde_json::to_string(component).unwrap_or_else(|_| "\"<unavailable>\"".into())
    }
}

fn dotted_path(path: &[String]) -> String {
    path.iter()
        .map(|component| display_path_component(component))
        .collect::<Vec<_>>()
        .join(".")
}

fn display_path_parts(path: &[String]) -> (String, String) {
    match path.split_last() {
        Some((leaf, parent)) => {
            let parent = if parent.is_empty() {
                String::new()
            } else {
                format!("{}.", dotted_path(parent))
            };
            (parent, display_path_component(leaf))
        }
        None => (String::new(), String::new()),
    }
}

fn observation_error(error: &ApiClientError) -> String {
    match error {
        ApiClientError::Status {
            code: 403 | 404, ..
        } => "This configuration observation is unavailable or you do not have access.".into(),
        ApiClientError::Status { code: 422, body } if !body.is_empty() => body.clone(),
        _ => "The configuration observation request failed.".into(),
    }
}

fn lifecycle_copy(lifecycle: ConfigObservationLifecycle) -> &'static str {
    match lifecycle {
        ConfigObservationLifecycle::Queued => "Queued",
        ConfigObservationLifecycle::WaitingForCapacity => "Waiting for evaluator capacity",
        ConfigObservationLifecycle::Running => "Inspecting",
        ConfigObservationLifecycle::Succeeded => "Loading observation",
        ConfigObservationLifecycle::Failed => "Inspection failed",
    }
}

fn render_json_value(value: &JsonValue) -> String {
    match value {
        JsonValue::String(value) => format!("\"{value}\""),
        _ => serde_json::to_string(value).unwrap_or_else(|_| "<unavailable>".into()),
    }
}

fn render_safe_option_value(value: &SafeOptionValue) -> String {
    match value {
        SafeOptionValue::Scalar(value) => render_json_value(value),
        SafeOptionValue::Package(package) => {
            let identity = package
                .name
                .as_ref()
                .or(package.pname.as_ref())
                .map(String::as_str)
                .unwrap_or("package");
            package
                .version
                .as_ref()
                .map(|version| format!("{identity}-{version}"))
                .unwrap_or_else(|| identity.to_string())
        }
        SafeOptionValue::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(render_safe_option_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SafeOptionValue::AttributeSet(values) | SafeOptionValue::Submodule(values) => {
            render_json_value(&JsonValue::Object(values.clone()))
        }
        SafeOptionValue::Opaque { type_name } => format!("<{type_name}: opaque>"),
        SafeOptionValue::Failed(error) => format!("not evaluated: {}", error.message),
    }
}

fn render_typed_diff_value(value: &JsonValue) -> String {
    serde_json::from_value::<SafeOptionValue>(value.clone())
        .map(|value| render_safe_option_value(&value))
        .unwrap_or_else(|_| render_json_value(value))
}

fn scoped_option<'a>(state: Option<&'a ObservationState>) -> Option<&'a ConfigObservationResponse> {
    match state {
        Some(ObservationState::Loaded(observation))
            if matches!(observation.payload, ConfigObservationPayload::Option { .. }) =>
        {
            Some(observation)
        }
        _ => None,
    }
}

fn scoped_option_value(state: Option<&ObservationState>) -> Option<String> {
    let observation = scoped_option(state)?;
    match &observation.payload {
        ConfigObservationPayload::Option { value, .. } => Some(render_safe_option_value(value)),
        _ => None,
    }
}

fn scoped_option_source(
    path: &[String],
    provenance: &HashMap<Vec<String>, ConfigObservationResponse>,
) -> Option<String> {
    match &provenance.get(path)?.payload {
        ConfigObservationPayload::Provenance { definitions, .. } => definitions
            .iter()
            .find_map(|definition| definition.source_path.clone()),
        _ => None,
    }
}

fn evaluated_option_source(option: &EvaluatedOption) -> Option<String> {
    option
        .definitions
        .iter()
        .find(|definition| definition.winning)
        .or_else(|| option.definitions.first())
        .and_then(|definition| {
            definition
                .source_path
                .clone()
                .or_else(|| definition.source_input.clone())
        })
}

fn observed_option_paths(
    root: &ObservationState,
    branches: &HashMap<Vec<String>, ObservationState>,
    configured: &ObservationState,
    details: &HashMap<Vec<String>, ObservationState>,
    provenance: &HashMap<Vec<String>, ConfigObservationResponse>,
    query: &str,
) -> Vec<Vec<String>> {
    let mut paths = BTreeMap::<Vec<String>, ()>::new();
    let mut add_children = |state: &ObservationState| {
        let ObservationState::Loaded(observation) = state else {
            return;
        };
        let children = match &observation.payload {
            ConfigObservationPayload::Root { children, .. }
            | ConfigObservationPayload::Prefix { children, .. } => children,
            _ => return,
        };
        for child in children {
            if child.kind == ConfigObservationChildKind::Option {
                paths.insert(child.path_components.clone(), ());
            }
        }
    };
    add_children(root);
    for branch in branches.values() {
        add_children(branch);
    }
    if let ObservationState::Loaded(observation) = configured
        && let ConfigObservationPayload::ConfiguredIndex {
            configured: options,
            ..
        } = &observation.payload
    {
        for option in options {
            paths.insert(option.path_components.clone(), ());
        }
    }
    for path in details.keys() {
        paths.insert(path.clone(), ());
    }

    let query = query.trim().to_ascii_lowercase();
    paths
        .into_keys()
        .filter(|path| {
            query.is_empty()
                || dotted_path(path).to_ascii_lowercase().contains(&query)
                || scoped_option_value(details.get(path))
                    .is_some_and(|value| value.to_ascii_lowercase().contains(&query))
                || scoped_option_source(path, provenance)
                    .is_some_and(|source| source.to_ascii_lowercase().contains(&query))
        })
        .collect()
}

fn observation_task_is_current(
    expected_scope: u64,
    current_scope: u64,
    expected_operation: u64,
    current_operation: u64,
) -> bool {
    expected_scope == current_scope && expected_operation == current_operation
}

fn merge_tree_observation(
    current: &ConfigObservationResponse,
    mut page: ConfigObservationResponse,
) -> Result<ConfigObservationResponse, String> {
    let (current_children, current_total) = match &current.payload {
        ConfigObservationPayload::Root {
            children,
            total_children,
            ..
        }
        | ConfigObservationPayload::Prefix {
            children,
            total_children,
            ..
        } => (children, *total_children),
        _ => return Err("Current observation is not a tree page.".into()),
    };
    let expected_offset = u32::try_from(current_children.len())
        .map_err(|_| "Current observation exceeds the child bound.".to_string())?;
    if page.child_offset != expected_offset
        || page.kind != current.kind
        || page.path_components != current.path_components
    {
        return Err("Continuation identity did not match the loaded tree.".into());
    }
    let known = current_children
        .iter()
        .map(|child| child.path_components.clone())
        .collect::<HashSet<_>>();
    let known_keys = current_children
        .iter()
        .map(|child| child.key.clone())
        .collect::<HashSet<_>>();
    let (page_children, page_total, page_truncated) = match &mut page.payload {
        ConfigObservationPayload::Root {
            child_offset,
            children,
            total_children,
            children_truncated,
            ..
        }
        | ConfigObservationPayload::Prefix {
            child_offset,
            children,
            total_children,
            children_truncated,
            ..
        } => {
            if *child_offset != expected_offset || *total_children != current_total {
                return Err("Continuation totals changed while loading the tree.".into());
            }
            (children, *total_children, *children_truncated)
        }
        _ => return Err("Continuation payload is not a tree page.".into()),
    };
    let mut page_paths = HashSet::new();
    let mut page_keys = HashSet::new();
    let mut previous_path = current_children
        .last()
        .map(|child| child.path_components.as_slice());
    for child in page_children.iter() {
        if known.contains(&child.path_components)
            || known_keys.contains(&child.key)
            || !page_paths.insert(child.path_components.clone())
            || !page_keys.insert(child.key.clone())
            || previous_path.is_some_and(|previous| previous >= child.path_components.as_slice())
        {
            return Err("Continuation repeated or reordered a child.".into());
        }
        previous_path = Some(&child.path_components);
    }
    let mut merged = current_children.clone();
    merged.append(page_children);
    let merged_count = u32::try_from(merged.len())
        .map_err(|_| "Merged observation exceeds the child bound.".to_string())?;
    let merged_count = u64::from(merged_count);
    if merged_count > page_total
        || page_truncated != (merged_count < page_total)
        || (page_truncated && merged_count == u64::from(expected_offset))
    {
        return Err("Continuation count did not match the loaded tree.".into());
    }
    match &mut page.payload {
        ConfigObservationPayload::Root {
            child_offset,
            children,
            total_children,
            children_truncated,
            ..
        }
        | ConfigObservationPayload::Prefix {
            child_offset,
            children,
            total_children,
            children_truncated,
            ..
        } => {
            *child_offset = 0;
            *children = merged;
            *total_children = page_total;
            *children_truncated = page_truncated;
        }
        _ => unreachable!("tree payload was checked above"),
    }
    page.child_offset = 0;
    Ok(page)
}

fn start_top_level_observation(
    system_id: Uuid,
    revision: String,
    kind: ConfigObservationKind,
    sequence: u64,
    operation_sequence: Signal<u64>,
    component_active: Rc<Cell<bool>>,
    mut state: Signal<ObservationState>,
) {
    state.set(ObservationState::Lifecycle(
        ConfigObservationLifecycle::Queued,
    ));
    spawn(async move {
        let is_current = || component_active.get() && sequence == *operation_sequence.peek();
        let result = load_system_config_observation(
            &system_id,
            &revision,
            CreateConfigObservationRequest {
                kind,
                path_components: Vec::new(),
                child_offset: 0,
            },
            |request| {
                if is_current() {
                    state.set(ObservationState::Lifecycle(request.lifecycle));
                }
            },
            is_current,
        )
        .await;
        if !is_current() {
            return;
        }
        match result {
            Ok(Some(observation)) => state.set(ObservationState::Loaded(observation)),
            Ok(None) => {}
            Err(error) => state.set(ObservationState::Error(observation_error(&error))),
        }
    });
}

#[component]
fn ExplorerStatus(state: ObservationState, surface: &'static str) -> Element {
    match state {
        ObservationState::Idle => rsx! {
            div { class: "cfg-explorer-status", role: "status", "{surface} is idle." }
        },
        ObservationState::Lifecycle(lifecycle) => rsx! {
            div { class: "cfg-explorer-status", role: "status", "{surface}: {lifecycle_copy(lifecycle)}" }
        },
        ObservationState::Error(error) => rsx! {
            div { class: "cfg-explorer-status cfg-explorer-error", role: "alert", "{surface}: {error}" }
        },
        ObservationState::Loaded(_) => rsx! {},
    }
}

#[component]
fn ExplorerChildren(
    entries: Vec<ConfigObservationChild>,
    depth: usize,
    expanded: Signal<HashSet<Vec<String>>>,
    branches: Signal<HashMap<Vec<String>, ObservationState>>,
    details: Signal<HashMap<Vec<String>, ObservationState>>,
    provenance: Signal<HashMap<Vec<String>, ConfigObservationResponse>>,
    more_loading: Signal<HashSet<Vec<String>>>,
    more_errors: Signal<HashMap<Vec<String>, String>>,
    on_prefix: EventHandler<(Vec<String>, u32)>,
    on_option: EventHandler<Vec<String>>,
) -> Element {
    rsx! {
        ul { class: "cfg-explorer-tree",
            for child in entries {
                {
                    let path = child.path_components.clone();
                    let display = dotted_path(&path);
                    let is_expanded = expanded.read().contains(&path);
                    let branch_state = branches.read().get(&path).cloned().unwrap_or(ObservationState::Idle);
                    let value = scoped_option_value(details.read().get(&path));
                    let source = scoped_option_source(&path, &provenance.read());
                    let value_label = value.as_deref().unwrap_or("—");
                    let source_label = source.as_deref().unwrap_or("—");
                    let row_style = format!("padding-left:{}px", 10 + depth * 15);
                    rsx! {
                        li { key: "{child.key}", class: "cfg-explorer-node",
                            match child.kind {
                                ConfigObservationChildKind::Prefix => rsx! {
                                    button {
                                        class: "cfg-explorer-tree-row focus-ring",
                                        style: "{row_style}",
                                        "aria-label": if is_expanded { format!("Collapse {display}") } else { format!("Expand {display}") },
                                        "aria-expanded": is_expanded,
                                        onclick: {
                                            let path = path.clone();
                                            move |_| on_prefix.call((path.clone(), 0))
                                        },
                                        span { class: "mono cfg-explorer-path", title: "{display}", span { class: if is_expanded { "cfg-caret open" } else { "cfg-caret" }, Icon { name: IconName::ChevronRight, size: 12 } } "{display}" }
                                        span { class: "cfgx-val mono", "—" }
                                        span { class: "cfgx-by mono", "—" }
                                    }
                                    if is_expanded {
                                        match branch_state.clone() {
                                            ObservationState::Loaded(observation) => match observation.payload {
                                                ConfigObservationPayload::Prefix { children, children_truncated, total_children, .. } => rsx! {
                                                    ExplorerChildren {
                                                        entries: children.clone(),
                                                        depth: depth + 1,
                                                        expanded,
                                                        branches,
                                                        details,
                                                        provenance,
                                                        more_loading,
                                                        more_errors,
                                                        on_prefix,
                                                        on_option,
                                                    }
                                                    if children_truncated {
                                                        button {
                                                            class: "btn btn-ghost focus-ring xs",
                                                            disabled: more_loading.read().contains(&path),
                                                            "aria-label": "Load more children under {display}",
                                                            onclick: {
                                                                let path = path.clone();
                                                                let offset = u32::try_from(children.len()).unwrap_or(u32::MAX);
                                                                move |event| { event.stop_propagation(); on_prefix.call((path.clone(), offset)); }
                                                            },
                                                            if more_loading.read().contains(&path) { "Loading more…" } else { "Load more" }
                                                        }
                                                        span { class: "cfg-explorer-local-note", role: "status", "Showing {children.len()} of {total_children} children under {display}." }
                                                    }
                                                    if let Some(error) = more_errors.read().get(&path) { div { class: "cfg-explorer-local-error", role: "alert", "{error}" } }
                                                },
                                                _ => rsx! { div { class: "cfg-explorer-local-error", role: "alert", "Unexpected observation payload for {display}." } },
                                            },
                                            ObservationState::Error(error) => rsx! {
                                                div { class: "cfg-explorer-local-error", role: "alert",
                                                    span { "{display}: {error}" }
                                                    button {
                                                        class: "btn btn-ghost focus-ring xs",
                                                        "aria-label": "Retry {display}",
                                                        onclick: {
                                                            let path = path.clone();
                                                            move |event| { event.stop_propagation(); on_prefix.call((path.clone(), 0)); }
                                                        },
                                                        "Retry"
                                                    }
                                                }
                                            },
                                            state => rsx! { ExplorerStatus { state, surface: "Prefix" } },
                                        }
                                    }
                                },
                                ConfigObservationChildKind::Option => rsx! {
                                    button {
                                        class: "cfg-explorer-tree-row cfg-explorer-option focus-ring",
                                        style: "{row_style}",
                                        "aria-label": "Inspect option {display}",
                                        onclick: {
                                            let path = path.clone();
                                            move |_| on_option.call(path.clone())
                                        },
                                        span { class: "mono cfg-explorer-path", title: "{display}", span { class: "cfg-explorer-leaf", Icon { name: IconName::File, size: 12 } } "{display}" }
                                        span { class: "cfgx-val mono", "{value_label}" }
                                        span { class: "cfgx-by mono", "{source_label}" }
                                    }
                                },
                                ConfigObservationChildKind::Unavailable => rsx! {
                                    div { class: "cfg-explorer-tree-row cfg-explorer-unavailable", style: "{row_style}", role: "status", "aria-label": "Unavailable child {display}",
                                        span { class: "mono cfg-explorer-path", title: "{display}", span { class: "cfg-explorer-leaf", Icon { name: IconName::Warn, size: 12 } } "{display}" }
                                        span { class: "cfgx-val mono cfg-val-err", "unavailable" }
                                        span { class: "cfgx-by mono", "—" }
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Renders independent lazy hierarchy and configured-option observations.
///
/// The component never uses the certified V2 snapshot token for scoped reads.
/// When `scoped_enabled` is false, Browse and Configured remain local
/// unavailable states while certified Search and Sources remain usable.
#[component]
pub(crate) fn ConfigExplorer(
    system_id: Uuid,
    revision: Option<String>,
    scoped_enabled: bool,
    disabled_reason: String,
    target: String,
    primary_lifecycle: SnapshotLifecycle,
    evaluation_time: String,
    package_count: String,
    closure_size: String,
    carrier: String,
    inventory_label: String,
    inventory_state: OptionInventoryState,
    inventory_request_allowed: bool,
    inventory_request_error: Option<String>,
    comparison_ready: bool,
    search: Signal<String>,
    search_rows: Vec<EvaluatedOptionRow>,
    search_total: i64,
    search_offset: i64,
    search_limit: i64,
    search_filter: EvaluatedOptionFilter,
    search_counts: EvaluatedOptionCounts,
    search_loading: bool,
    search_error: Option<String>,
    comparison_baseline: Option<String>,
    certified_sources: Vec<EvaluationModuleSummary>,
    certified_sources_complete: bool,
    certified_sources_has_more: bool,
    certified_sources_loading_more: bool,
    certified_sources_error: Option<String>,
    on_request_inventory: EventHandler<()>,
    on_search_offset: EventHandler<i64>,
    on_search_filter: EventHandler<EvaluatedOptionFilter>,
    on_load_more_sources: EventHandler<()>,
    on_open_definition: EventHandler<OptionDefinitionProvenance>,
    on_open_source: EventHandler<EvaluationModuleSummary>,
) -> Element {
    let mut scope_sequence = use_signal(|| 0_u64);
    let mut root_sequence = use_signal(|| 0_u64);
    let mut configured_sequence = use_signal(|| 0_u64);
    let mut branch_sequence = use_signal(|| 0_u64);
    let mut detail_sequence = use_signal(|| 0_u64);
    let mut provenance_sequence = use_signal(|| 0_u64);
    let mut root = use_signal(|| ObservationState::Idle);
    let mut configured = use_signal(|| ObservationState::Idle);
    let mut branches = use_signal(HashMap::<Vec<String>, ObservationState>::new);
    let mut branch_sequences = use_signal(HashMap::<Vec<String>, u64>::new);
    let mut branch_more_loading = use_signal(HashSet::<Vec<String>>::new);
    let mut branch_more_errors = use_signal(HashMap::<Vec<String>, String>::new);
    let mut expanded = use_signal(HashSet::<Vec<String>>::new);
    let mut root_more_loading = use_signal(|| false);
    let mut root_more_error = use_signal(|| None::<String>);
    let mut detail = use_signal(|| ObservationState::Idle);
    let mut detail_cache = use_signal(HashMap::<Vec<String>, ObservationState>::new);
    let mut detail_path = use_signal(Vec::<String>::new);
    let mut certified_detail = use_signal(|| None::<EvaluatedOptionRow>);
    let mut provenance = use_signal(|| ObservationState::Idle);
    let mut provenance_cache = use_signal(HashMap::<Vec<String>, ConfigObservationResponse>::new);
    let mut mode = use_signal(|| ExplorerMode::Browse);
    let mut inspector_pane = use_signal(|| InspectorPane::Sources);
    let component_active = use_hook(|| Rc::new(Cell::new(true)));
    {
        let component_active = component_active.clone();
        use_drop(move || component_active.set(false));
    }

    {
        let component_active = component_active.clone();
        use_effect(use_reactive(
            (&revision, &scoped_enabled),
            move |(requested_revision, scoped_enabled)| {
                // CONCURRENCY: Invalidate every prior operation before clearing
                // scoped state. Start Root in this effect so a separate reset
                // effect cannot overwrite its Queued lifecycle with Idle.
                let sequence = next_operation_sequence(*scope_sequence.peek());
                scope_sequence.set(sequence);
                let root_operation = next_operation_sequence(*root_sequence.peek());
                root_sequence.set(root_operation);
                let configured_operation = next_operation_sequence(*configured_sequence.peek());
                configured_sequence.set(configured_operation);
                branches.set(HashMap::new());
                branch_sequences.set(HashMap::new());
                branch_more_loading.set(HashSet::new());
                branch_more_errors.set(HashMap::new());
                expanded.set(HashSet::new());
                root.set(ObservationState::Idle);
                configured.set(ObservationState::Idle);
                root_more_loading.set(false);
                root_more_error.set(None);
                detail.set(ObservationState::Idle);
                detail_cache.set(HashMap::new());
                detail_path.set(Vec::new());
                certified_detail.set(None);
                provenance.set(ObservationState::Idle);
                provenance_cache.set(HashMap::new());
                mode.set(ExplorerMode::Browse);
                inspector_pane.set(InspectorPane::Sources);
                let operations =
                    initial_scoped_operations(requested_revision.as_deref(), scoped_enabled);
                let Some(revision) = requested_revision else {
                    return;
                };
                for operation in operations {
                    start_top_level_observation(
                        system_id,
                        revision.clone(),
                        operation,
                        root_operation,
                        root_sequence,
                        component_active.clone(),
                        root,
                    );
                }
            },
        ));
    }

    {
        let revision = revision.clone();
        let component_active = component_active.clone();
        use_effect(use_reactive(&mode, move |selected_mode| {
            if !should_start_configured(*selected_mode.read(), &configured.peek()) {
                return;
            }
            let sequence = next_operation_sequence(*configured_sequence.peek());
            configured_sequence.set(sequence);
            let Some(revision) = revision.clone().filter(|_| scoped_enabled) else {
                configured.set(ObservationState::Idle);
                return;
            };
            start_top_level_observation(
                system_id,
                revision,
                ConfigObservationKind::ConfiguredIndex,
                sequence,
                configured_sequence,
                component_active.clone(),
                configured,
            );
        }));
    }

    let retry_root = EventHandler::new({
        let revision = revision.clone();
        let component_active = component_active.clone();
        move |_| {
            let Some(revision) = revision.clone().filter(|_| scoped_enabled) else {
                return;
            };
            let sequence = next_operation_sequence(*root_sequence.peek());
            root_sequence.set(sequence);
            start_top_level_observation(
                system_id,
                revision,
                ConfigObservationKind::Root,
                sequence,
                root_sequence,
                component_active.clone(),
                root,
            );
        }
    });
    let retry_configured = EventHandler::new({
        let revision = revision.clone();
        let component_active = component_active.clone();
        move |_| {
            let Some(revision) = revision.clone().filter(|_| scoped_enabled) else {
                return;
            };
            let sequence = next_operation_sequence(*configured_sequence.peek());
            configured_sequence.set(sequence);
            start_top_level_observation(
                system_id,
                revision,
                ConfigObservationKind::ConfiguredIndex,
                sequence,
                configured_sequence,
                component_active.clone(),
                configured,
            );
        }
    });

    let load_prefix = EventHandler::new({
        let revision = revision.clone();
        let component_active = component_active.clone();
        move |(path, child_offset): (Vec<String>, u32)| {
            let currently_expanded = expanded.peek().contains(&path);
            let loaded = matches!(
                branches.peek().get(&path),
                Some(ObservationState::Loaded(_))
            );
            if child_offset == 0 && currently_expanded && loaded {
                expanded.write().remove(&path);
                return;
            }
            expanded.write().insert(path.clone());
            if !should_request_prefix(child_offset, loaded) {
                return;
            }
            let Some(revision) = revision.clone().filter(|_| scoped_enabled) else {
                return;
            };
            let sequence = branch_sequence.peek().saturating_add(1);
            branch_sequence.set(sequence);
            let scope = *scope_sequence.peek();
            branch_sequences.write().insert(path.clone(), sequence);
            if child_offset == 0 {
                branches.write().insert(
                    path.clone(),
                    ObservationState::Lifecycle(ConfigObservationLifecycle::Queued),
                );
            } else {
                branch_more_loading.write().insert(path.clone());
                branch_more_errors.write().remove(&path);
            }
            let component_active = component_active.clone();
            spawn(async move {
                let is_current = || {
                    component_active.get()
                        && observation_task_is_current(
                            scope,
                            *scope_sequence.peek(),
                            sequence,
                            branch_sequences
                                .peek()
                                .get(&path)
                                .copied()
                                .unwrap_or_default(),
                        )
                };
                let result = load_system_config_observation(
                    &system_id,
                    &revision,
                    CreateConfigObservationRequest {
                        kind: ConfigObservationKind::Prefix,
                        path_components: path.clone(),
                        child_offset,
                    },
                    |request| {
                        if is_current() && child_offset == 0 {
                            branches.write().insert(
                                path.clone(),
                                ObservationState::Lifecycle(request.lifecycle),
                            );
                        }
                    },
                    is_current,
                )
                .await;
                if is_current() {
                    let next_state = match result {
                        Ok(Some(observation)) if child_offset > 0 => {
                            let current = branches.peek().get(&path).cloned();
                            match current {
                                Some(ObservationState::Loaded(current)) => {
                                    match merge_tree_observation(&current, observation) {
                                        Ok(observation) => ObservationState::Loaded(observation),
                                        Err(error) => {
                                            branch_more_errors.write().insert(path.clone(), error);
                                            ObservationState::Loaded(current)
                                        }
                                    }
                                }
                                _ => ObservationState::Error(
                                    "Loaded tree was replaced before continuation completed."
                                        .into(),
                                ),
                            }
                        }
                        Ok(Some(observation)) => ObservationState::Loaded(observation),
                        Ok(None) => return,
                        Err(error) if child_offset > 0 => {
                            branch_more_errors
                                .write()
                                .insert(path.clone(), observation_error(&error));
                            branch_more_loading.write().remove(&path);
                            return;
                        }
                        Err(error) => ObservationState::Error(observation_error(&error)),
                    };
                    branch_more_loading.write().remove(&path);
                    branches.write().insert(path, next_state);
                }
            });
        }
    });

    let select_option = EventHandler::new({
        let revision = revision.clone();
        let component_active = component_active.clone();
        move |path: Vec<String>| {
            certified_detail.set(None);
            detail_path.set(path.clone());
            inspector_pane.set(InspectorPane::Option);
            provenance.set(
                provenance_cache
                    .peek()
                    .get(&path)
                    .cloned()
                    .map(ObservationState::Loaded)
                    .unwrap_or(ObservationState::Idle),
            );
            if let Some(cached) = detail_cache.peek().get(&path).cloned() {
                detail.set(cached);
                return;
            }
            let Some(revision) = revision.clone().filter(|_| scoped_enabled) else {
                return;
            };
            let sequence = detail_sequence.peek().saturating_add(1);
            detail_sequence.set(sequence);
            let scope = *scope_sequence.peek();
            detail.set(ObservationState::Lifecycle(
                ConfigObservationLifecycle::Queued,
            ));
            let component_active = component_active.clone();
            spawn(async move {
                let is_current = || {
                    component_active.get()
                        && observation_task_is_current(
                            scope,
                            *scope_sequence.peek(),
                            sequence,
                            *detail_sequence.peek(),
                        )
                        && detail_path.peek().as_slice() == path.as_slice()
                };
                let result = load_system_config_observation(
                    &system_id,
                    &revision,
                    CreateConfigObservationRequest {
                        kind: ConfigObservationKind::Option,
                        path_components: path.clone(),
                        child_offset: 0,
                    },
                    |request| {
                        if is_current() {
                            detail.set(ObservationState::Lifecycle(request.lifecycle));
                        }
                    },
                    is_current,
                )
                .await;
                if is_current() {
                    detail.set(match result {
                        Ok(Some(observation)) => {
                            let loaded = ObservationState::Loaded(observation);
                            detail_cache.write().insert(path.clone(), loaded.clone());
                            loaded
                        }
                        Ok(None) => return,
                        Err(error) => ObservationState::Error(observation_error(&error)),
                    });
                }
            });
        }
    });

    let load_provenance = {
        let revision = revision.clone();
        let component_active = component_active.clone();
        move |_| {
            let path = detail_path.peek().clone();
            let Some(revision) = revision
                .clone()
                .filter(|_| scoped_enabled && !path.is_empty())
            else {
                return;
            };
            let sequence = provenance_sequence.peek().saturating_add(1);
            provenance_sequence.set(sequence);
            let scope = *scope_sequence.peek();
            provenance.set(ObservationState::Lifecycle(
                ConfigObservationLifecycle::Queued,
            ));
            let component_active = component_active.clone();
            spawn(async move {
                let is_current = || {
                    component_active.get()
                        && observation_task_is_current(
                            scope,
                            *scope_sequence.peek(),
                            sequence,
                            *provenance_sequence.peek(),
                        )
                        && detail_path.peek().as_slice() == path.as_slice()
                };
                let result = load_system_config_observation(
                    &system_id,
                    &revision,
                    CreateConfigObservationRequest {
                        kind: ConfigObservationKind::Provenance,
                        path_components: path.clone(),
                        child_offset: 0,
                    },
                    |request| {
                        if is_current() {
                            provenance.set(ObservationState::Lifecycle(request.lifecycle));
                        }
                    },
                    is_current,
                )
                .await;
                if is_current() {
                    provenance.set(match result {
                        Ok(Some(observation)) => {
                            provenance_cache
                                .write()
                                .insert(path.clone(), observation.clone());
                            ObservationState::Loaded(observation)
                        }
                        Ok(None) => return,
                        Err(error) => ObservationState::Error(observation_error(&error)),
                    });
                }
            });
        }
    };

    let load_more_root = {
        let revision = revision.clone();
        let component_active = component_active.clone();
        move |_| {
            let current = root.peek().clone();
            let ObservationState::Loaded(current_observation) = current else {
                return;
            };
            let ConfigObservationPayload::Root { children, .. } = &current_observation.payload
            else {
                return;
            };
            let Ok(child_offset) = u32::try_from(children.len()) else {
                root_more_error.set(Some("Loaded root exceeds the continuation bound.".into()));
                return;
            };
            let Some(revision) = revision.clone().filter(|_| scoped_enabled) else {
                return;
            };
            let sequence = root_sequence.peek().saturating_add(1);
            root_sequence.set(sequence);
            let scope = *scope_sequence.peek();
            root_more_loading.set(true);
            root_more_error.set(None);
            let component_active = component_active.clone();
            spawn(async move {
                let is_current = || {
                    component_active.get()
                        && observation_task_is_current(
                            scope,
                            *scope_sequence.peek(),
                            sequence,
                            *root_sequence.peek(),
                        )
                };
                let result = load_system_config_observation(
                    &system_id,
                    &revision,
                    CreateConfigObservationRequest {
                        kind: ConfigObservationKind::Root,
                        path_components: Vec::new(),
                        child_offset,
                    },
                    |_| {},
                    is_current,
                )
                .await;
                if !is_current() {
                    return;
                }
                root_more_loading.set(false);
                match result {
                    Ok(Some(page)) => match merge_tree_observation(&current_observation, page) {
                        Ok(observation) => root.set(ObservationState::Loaded(observation)),
                        Err(error) => root_more_error.set(Some(error)),
                    },
                    Ok(None) => {}
                    Err(error) => root_more_error.set(Some(observation_error(&error))),
                }
            });
        }
    };

    let select_certified = EventHandler::new(move |row: EvaluatedOptionRow| {
        let next_detail = detail_sequence.peek().saturating_add(1);
        let next_provenance = provenance_sequence.peek().saturating_add(1);
        detail_sequence.set(next_detail);
        provenance_sequence.set(next_provenance);
        detail_path.set(Vec::new());
        detail.set(ObservationState::Idle);
        provenance.set(ObservationState::Idle);
        certified_detail.set(Some(row));
        inspector_pane.set(InspectorPane::Option);
    });

    let root_state = root.read().clone();
    let configured_state = configured.read().clone();
    let detail_state = detail.read().clone();
    let provenance_state = provenance.read().clone();
    let certified_detail_state = certified_detail.read().clone();
    let selected_mode = *mode.read();
    let selected_pane = *inspector_pane.read();
    let inventory_complete = inventory_state == OptionInventoryState::Complete;
    let local_search_paths = observed_option_paths(
        &root_state,
        &branches.read(),
        &configured_state,
        &detail_cache.read(),
        &provenance_cache.read(),
        &search.read(),
    );
    let observed_sources = provenance_cache
        .read()
        .values()
        .filter_map(|observation| match &observation.payload {
            ConfigObservationPayload::Provenance { definitions, .. } => Some(definitions),
            _ => None,
        })
        .flatten()
        .filter_map(|definition| definition.source_path.clone())
        .fold(BTreeMap::<String, usize>::new(), |mut sources, path| {
            *sources.entry(path).or_default() += 1;
            sources
        });
    let primary_label = match primary_lifecycle {
        SnapshotLifecycle::Available => "complete",
        SnapshotLifecycle::Queued => "queued",
        SnapshotLifecycle::Running => "running",
        SnapshotLifecycle::Failed => "failed",
        SnapshotLifecycle::Unavailable => "unavailable",
    };
    let search_page_end = search_offset
        .saturating_add(i64::try_from(search_rows.len()).unwrap_or(i64::MAX))
        .min(search_total);
    let changed_count_label = search_counts
        .changed
        .map(|count| count.to_string())
        .unwrap_or_else(|| "unavailable".into());
    rsx! {
        div { class: "cfgx-explorer",
            div { class: "cfgx-meta",
                div { class: "cfgx-meta-i", title: "Whether the primary evaluator has produced a result for this exact target. Explorer observations never replace primary evaluation.", span { "primary eval" } b { class: if primary_lifecycle == SnapshotLifecycle::Available { "ok" } else { "warn" }, "{primary_label}" } }
                div { class: "cfgx-meta-i", span { "eval time" } b { class: "mono", "{evaluation_time}" } }
                div { class: "cfgx-meta-i", span { "packages" } b { class: "mono", "{package_count}" } }
                div { class: "cfgx-meta-i", span { "closure" } b { class: "mono", "{closure_size}" } }
                div { class: "cfgx-meta-i", span { "carrier" } b { class: "mono", title: "{carrier}", "{carrier}" } }
                div { class: "cfgx-meta-sp" }
                div { class: "cfgx-meta-i", title: "Browsing needs no complete inventory. A complete certified inventory enables complete search and comparison.", span { "inventory" }
                    b { class: if inventory_complete { "ok" } else { "warn" }, "{inventory_label}" }
                    if inventory_request_allowed {
                        button { class: "cfgx-link focus-ring", onclick: move |_| on_request_inventory.call(()), "request full inventory" }
                    }
                }
                div { class: "cfgx-meta-i", title: if comparison_ready { "A certified complete inventory and valid baseline make comparison meaningful." } else { "Changed and Drift require sufficient certified complete coverage. Incomplete data does not mean zero changes or no drift." }, span { "comparison" } b { class: if comparison_ready { "ok" } else { "warn" }, if comparison_ready { "ready" } else { "unavailable" } } }
            }
                if let Some(error) = inventory_request_error.as_deref() {
                    div { class: "cfg-explorer-local-error", role: "alert", "Configuration inspection prerequisite: {error}" }
                }
                div { class: "cfgx-tools",
                    div { class: "seg xs",
                        button { class: if selected_mode == ExplorerMode::Browse { "active focus-ring" } else { "focus-ring" }, "aria-pressed": selected_mode == ExplorerMode::Browse, onclick: move |_| mode.set(ExplorerMode::Browse), "Browse" }
                        button { class: if selected_mode == ExplorerMode::Configured { "active focus-ring" } else { "focus-ring" }, "aria-pressed": selected_mode == ExplorerMode::Configured, onclick: move |_| mode.set(ExplorerMode::Configured), "Configured" }
                        button { class: if selected_mode == ExplorerMode::Search { "active focus-ring" } else { "focus-ring" }, "aria-pressed": selected_mode == ExplorerMode::Search, onclick: move |_| mode.set(ExplorerMode::Search), "Search" }
                    }
                    div { class: "cfgx-search", Icon { name: IconName::Search, size: 12 }
                        input { class: "focus-ring", value: "{search}", placeholder: if inventory_complete { "Search all certified options…" } else { "Search observed options…" }, oninput: move |event| { search.set(event.value()); mode.set(ExplorerMode::Search); } }
                        if !search.read().is_empty() { button { class: "btn-icon xs focus-ring", title: "Clear", "aria-label": "Clear option search", onclick: move |_| search.set(String::new()), Icon { name: IconName::X, size: 12 } } }
                    }
                    if selected_mode == ExplorerMode::Search && inventory_complete {
                        div { class: "seg xs", "aria-label": "Certified option filter",
                            button { class: if search_filter == EvaluatedOptionFilter::All { "active focus-ring" } else { "focus-ring" }, "aria-pressed": search_filter == EvaluatedOptionFilter::All, onclick: move |_| on_search_filter.call(EvaluatedOptionFilter::All), "All ({search_counts.all})" }
                            button { class: if search_filter == EvaluatedOptionFilter::Overridden { "active focus-ring" } else { "focus-ring" }, "aria-pressed": search_filter == EvaluatedOptionFilter::Overridden, onclick: move |_| on_search_filter.call(EvaluatedOptionFilter::Overridden), "Overridden ({search_counts.overridden})" }
                            button { class: if search_filter == EvaluatedOptionFilter::Changed { "active focus-ring" } else { "focus-ring" }, "aria-pressed": search_filter == EvaluatedOptionFilter::Changed, disabled: !comparison_ready, title: if comparison_ready { "Compare with the selected mode's valid baseline." } else { "Changed requires a valid certified comparison baseline." }, onclick: move |_| on_search_filter.call(EvaluatedOptionFilter::Changed), "Changed ({changed_count_label})" }
                        }
                    }
                    span { class: "cfgx-count mono", if selected_mode == ExplorerMode::Search { if inventory_complete && search_loading { "searching…" } else if inventory_complete { "{search_total} hits" } else { "{local_search_paths.len()} observed" } } }
                }
                if selected_mode == ExplorerMode::Search {
                    div { class: if inventory_complete { "cfgx-scope full" } else { "cfgx-scope partial" },
                        if inventory_complete { "Complete search over the certified inventory for this exact target." }
                        else { "Partial search over options observed in this Explorer session only. No match does not mean the option is absent." }
                    }
                }
                div { class: "cfgx-body",
                    section { class: "cfgx-tree-col", "aria-label": "Configuration options",
                        div { class: "cfgx-colhead", span { if selected_mode == ExplorerMode::Configured { "configured option" } else if selected_mode == ExplorerMode::Search { "match" } else { "config.*" } } span { "value" } span { "defined by" } }
                        div { class: "cfgx-scroll",
                        if selected_mode == ExplorerMode::Browse {
                        if !scoped_enabled {
                            div { class: "cfgx-scope partial", role: "status", "{disabled_reason}" }
                        } else { match root_state.clone() {
                            ObservationState::Loaded(observation) => match observation.payload {
                                ConfigObservationPayload::Root { children, children_truncated, total_children, .. } => rsx! {
                                        ExplorerChildren { entries: children.clone(), depth: 0, expanded, branches, details: detail_cache, provenance: provenance_cache, more_loading: branch_more_loading, more_errors: branch_more_errors, on_prefix: load_prefix, on_option: select_option }
                                    if children_truncated {
                                        div { class: "cfgx-more", span { role: "status", "Showing {children.len()} of {total_children}" } button { class: "cfgx-link focus-ring", disabled: *root_more_loading.read(), onclick: load_more_root, if *root_more_loading.read() { "loading…" } else { "load more" } } }
                                    }
                                    if let Some(error) = root_more_error.read().as_ref() { div { class: "cfg-explorer-local-error", role: "alert", "{error}" } }
                                },
                                _ => rsx! { div { class: "cfg-explorer-local-error", role: "alert", "Unexpected root observation payload." } },
                            },
                            ObservationState::Error(error) => rsx! {
                                div { class: "cfg-explorer-local-error", role: "alert",
                                    span { "Root: {error}" }
                                    button {
                                        class: "btn btn-ghost focus-ring xs",
                                        "aria-label": "Retry root configuration observation",
                                        onclick: retry_root,
                                        "Retry"
                                    }
                                }
                            },
                            state => rsx! { ExplorerStatus { state, surface: "Root" } },
                        } }
                        } else if selected_mode == ExplorerMode::Configured {
                        if !scoped_enabled {
                            div { class: "cfgx-scope partial", role: "status", "{disabled_reason}" }
                        } else { match configured_state.clone() {
                            ObservationState::Loaded(observation) => match observation.payload {
                                ConfigObservationPayload::ConfiguredIndex {
                                    configured: options,
                                    total_traversed,
                                    diagnostics,
                                    diagnostics_truncated,
                                    total_configured,
                                    configured_truncated,
                                    classifier_diagnostics,
                                    classifier_diagnostics_truncated,
                                    ..
                                } => rsx! {
                                    div { class: "cfg-explorer-configured-meta", role: "status", "{total_configured} configured from {total_traversed} traversed; declaration-only defaults excluded" }
                                    if options.is_empty() { div { class: "cfg-explorer-status", "No surviving non-default assignments were observed." } }
                                    else { ConfiguredOptions { options, details: detail_cache, provenance: provenance_cache, on_option: select_option } }
                                    if configured_truncated { div { class: "cfg-explorer-local-note", "The configured list is bounded; {total_configured} identities exist." } }
                                    if !diagnostics.is_empty() || !classifier_diagnostics.is_empty() || diagnostics_truncated || classifier_diagnostics_truncated {
                                        div { class: "cfg-explorer-diagnostics", role: "status", "Inspection retained {diagnostics.len()} traversal and {classifier_diagnostics.len()} classifier diagnostics. Some identities may be unavailable." }
                                    }
                                },
                                _ => rsx! { div { class: "cfg-explorer-local-error", role: "alert", "Unexpected configured-index payload." } },
                            },
                            ObservationState::Error(error) => rsx! {
                                div { class: "cfg-explorer-local-error", role: "alert",
                                    span { "Configured options: {error}" }
                                    button {
                                        class: "btn btn-ghost focus-ring xs",
                                        "aria-label": "Retry configured options observation",
                                        onclick: retry_configured,
                                        "Retry"
                                    }
                                }
                            },
                            state => rsx! { ExplorerStatus { state, surface: "Configured options" } },
                        } }
                        } else if inventory_complete {
                            if search_loading {
                                div { class: "cfg-explorer-status", role: "status", "Searching cached certified data…" }
                            } else if let Some(error) = search_error.as_deref() {
                                div { class: "cfg-explorer-local-error", role: "alert", "Search: {error}" }
                            } else if search_rows.is_empty() {
                                div { class: "cfg-explorer-status", "No options match." }
                            } else {
                                ul { class: "cfg-explorer-configured",
                                    for row in search_rows {
                                        if let Some(option) = row.option.as_ref().or(row.before.as_ref()) {
                                            {
                                                let path = option.path.clone();
                                                let value = row.option.as_ref().map(|selected| render_safe_option_value(&selected.value)).unwrap_or_else(|| "removed".into());
                                                let source = row.option.as_ref().or(row.before.as_ref()).and_then(evaluated_option_source).unwrap_or_else(|| "—".into());
                                                let selected = row.clone();
                                                rsx! { li { key: "{path}", button { class: "cfgx-row hit focus-ring", "aria-label": "Inspect certified option {path}", onclick: move |_| select_certified.call(selected.clone()), span { class: "cfgx-name mono", title: "{path}", "{path}" } span { class: "cfgx-val mono", title: "{value}", "{value}" } span { class: "cfgx-by mono", title: "{source}", "{source}" } } } }
                                            }
                                        }
                                    }
                                }
                                div { class: "cfgx-more", role: "navigation", "aria-label": "Certified option search pages",
                                    span { role: "status", if search_total > 0 { "Showing {search_offset + 1}–{search_page_end} of {search_total}" } else { "No results" } }
                                    button { class: "cfgx-link focus-ring", disabled: search_offset <= 0 || search_loading, onclick: move |_| on_search_offset.call(search_offset.saturating_sub(search_limit)), "previous" }
                                    button { class: "cfgx-link focus-ring", disabled: search_offset.saturating_add(search_limit) >= search_total || search_loading, onclick: move |_| on_search_offset.call(search_offset.saturating_add(search_limit)), "next" }
                                }
                            }
                        } else if local_search_paths.is_empty() {
                            div { class: "cfg-explorer-status", "No match in options observed during this Explorer session. Unobserved paths were not searched." }
                        } else {
                            ul { class: "cfg-explorer-configured",
                                for path in local_search_paths {
                                    {
                                        let display = dotted_path(&path);
                                        let value = scoped_option_value(detail_cache.read().get(&path)).unwrap_or_else(|| "—".into());
                                        let source = scoped_option_source(&path, &provenance_cache.read()).unwrap_or_else(|| "—".into());
                                        rsx! { li { key: "{display}", button { class: "cfgx-row hit focus-ring", "aria-label": "Inspect observed option {display}", onclick: { let path = path.clone(); move |_| select_option.call(path.clone()) }, span { class: "cfgx-name mono", title: "{display}", "{display}" } span { class: "cfgx-val mono", title: "{value}", "{value}" } span { class: "cfgx-by mono", title: "{source}", "{source}" } } } }
                                    }
                                }
                            }
                        }
                        }
                    }
                    aside { class: "cfgx-side", "aria-label": "Configuration inspector",
                        div { class: "cfgx-side-tabs seg xs",
                            button { class: if selected_pane == InspectorPane::Option { "active focus-ring" } else { "focus-ring" }, "aria-pressed": selected_pane == InspectorPane::Option, disabled: detail_path.read().is_empty() && certified_detail.read().is_none(), onclick: move |_| inspector_pane.set(InspectorPane::Option), "Option" }
                            button { class: if selected_pane == InspectorPane::Sources { "active focus-ring" } else { "focus-ring" }, "aria-pressed": selected_pane == InspectorPane::Sources, onclick: move |_| inspector_pane.set(InspectorPane::Sources), "Sources" }
                        }
                    if selected_pane == InspectorPane::Option {
                    if let Some(row) = certified_detail_state {
                        CertifiedOptionDetail { row, comparison_baseline: comparison_baseline.clone(), on_open_definition }
                    } else { match detail_state {
                        ObservationState::Idle => rsx! { div { class: "cfg-explorer-detail-empty", "Select an option from either pane to inspect the same lazy detail." } },
                        ObservationState::Loaded(observation) => match observation.payload {
                            ConfigObservationPayload::Option { path_components, declared_type, is_defined, highest_prio, value, .. } => {
                                let path = dotted_path(&path_components);
                                let value_text = render_safe_option_value(&value);
                                let declared_type_label = declared_type.as_deref().unwrap_or("Unavailable");
                                let defined_label = if is_defined { "yes" } else { "no" };
                                let priority_label = highest_prio.map(|value| value.to_string()).unwrap_or_else(|| "Unavailable".into());
                                rsx! {
                                    { let (parent, leaf) = display_path_parts(&path_components); rsx! { div { class: "cfgx-insp-head", div { class: "mono cfgx-insp-path", span { class: "dim", "config.{parent}" } "{leaf}" } } } }
                                    dl { class: "cfg-explorer-facts",
                                        div { dt { "Declared type" } dd { class: "mono", "{declared_type_label}" } }
                                        div { dt { "Safe value" } dd { class: if matches!(value, SafeOptionValue::Failed(_)) { "mono cfg-val-err" } else { "mono" }, "{value_text}" } }
                                        div { dt { "Defined" } dd { "{defined_label}" } }
                                        div { dt { "Highest priority" } dd { class: "mono", "{priority_label}" } }
                                    }
                                    p { class: "cfg-explorer-copy", "Basic detail does not imply complete provenance." }
                                    if matches!(provenance_state, ObservationState::Idle | ObservationState::Error(_)) {
                                        button { class: "cfgx-btn focus-ring", "aria-label": "Inspect provenance for {path}", onclick: load_provenance, "Inspect provenance" }
                                    }
                                    match provenance_state {
                                        ObservationState::Loaded(observation) => match observation.payload {
                                            ConfigObservationPayload::Provenance { definitions, definitions_truncated, total_definitions, .. } => rsx! {
                                                div { class: "cfg-explorer-provenance",
                                                    h4 { "Observed definitions" }
                                                    if definitions.is_empty() { p { "No definitions were observed." } }
                                                    for definition in definitions {
                                                        {
                                                            let source_label = definition.source_path.as_deref().unwrap_or("Source unavailable");
                                                            let priority_label = definition.priority.map(|value| value.to_string()).unwrap_or_else(|| "unknown".into());
                                                            rsx! { div { class: "cfg-def", span { class: "mono cfg-def-file", "{source_label}" } span { class: "cfg-def-note mono", "priority {priority_label}" } } }
                                                        }
                                                    }
                                                    if definitions_truncated { p { class: "cfg-explorer-local-note", "Showing bounded definitions; {total_definitions} exist." } }
                                                }
                                            },
                                            _ => rsx! { div { class: "cfg-explorer-local-error", role: "alert", "Unexpected provenance payload." } },
                                        },
                                        ObservationState::Error(error) => rsx! { div { class: "cfg-explorer-local-error", role: "alert", "Provenance: {error}" } },
                                        state if state != ObservationState::Idle => rsx! { ExplorerStatus { state, surface: "Provenance" } },
                                        _ => rsx! {},
                                    }
                                }
                            },
                            _ => rsx! { div { class: "cfg-explorer-local-error", role: "alert", "Unexpected option observation payload." } },
                        },
                        ObservationState::Error(error) => rsx! { div { class: "cfg-explorer-local-error", role: "alert", "Option detail: {error}" } },
                        state => rsx! { ExplorerStatus { state, surface: "Option detail" } },
                    } }
                    } else {
                        div { class: "cfgx-side-hint",
                            if certified_sources_complete { "Complete certified source paths for this target. Explorer-observed paths are included in the same target view." }
                            else { "Partial list: only source paths from provenance inspected in this Explorer target and session. This is not a complete module registry." }
                        }
                        div { class: "cfgx-mods",
                            for source in &certified_sources {
                                {
                                    let source_label = source.source_path.as_deref().or(source.source_input.as_deref()).unwrap_or("Source path unavailable");
                                    let source_available = source.source_path.is_some() || source.tracked_flake.is_some();
                                    rsx! { button { class: "cfgx-mod focus-ring", disabled: !source_available, "aria-label": "Inspect source {source_label}", onclick: { let source = source.clone(); move |_| on_open_source.call(source.clone()) }, span { class: "mono cfgx-mod-p", title: "{source_label}", "{source_label}" } span { class: "mono cfgx-mod-n", "{source.defined_count}" } } }
                                }
                            }
                            if certified_sources_loading_more && certified_sources.is_empty() {
                                div { class: "cfg-explorer-status", role: "status", "Loading certified sources…" }
                            } else if certified_sources.is_empty() && observed_sources.is_empty() {
                                div { class: "cfg-explorer-status", "Nothing observed yet. Select an option and Inspect provenance." }
                            }
                            for (path, count) in observed_sources {
                                if !certified_sources.iter().any(|source| source.source_path.as_deref() == Some(path.as_str())) {
                                    div { class: "cfgx-mod", span { class: "mono cfgx-mod-p", title: "{path}", "{path}" } span { class: "mono cfgx-mod-n", "{count}" } }
                                }
                            }
                            if certified_sources_has_more {
                                button { class: "cfgx-link cfgx-source-more focus-ring", disabled: certified_sources_loading_more, onclick: move |_| on_load_more_sources.call(()), if certified_sources_loading_more { "loading…" } else { "load more certified sources" } }
                            }
                            if let Some(error) = certified_sources_error.as_deref() {
                                div { class: "cfg-explorer-local-error", role: "alert", "Sources: {error}" }
                            }
                        }
                    }
                    }
                }
            div { class: "cfgx-foot mono", "observational cache · {target}" }
        }
    }
}

#[component]
fn CertifiedOptionDetail(
    row: EvaluatedOptionRow,
    comparison_baseline: Option<String>,
    on_open_definition: EventHandler<OptionDefinitionProvenance>,
) -> Element {
    let Some(option) = row.option.as_ref().or(row.before.as_ref()) else {
        return rsx! { div { class: "cfg-explorer-detail-empty", "Certified option detail is unavailable." } };
    };
    let value = row
        .option
        .as_ref()
        .map(|selected| render_safe_option_value(&selected.value))
        .unwrap_or_else(|| "removed".into());
    let declared_type = option.declared_type.as_deref().unwrap_or("Unavailable");
    let overridden = option
        .overridden
        .map(|value| if value { "yes" } else { "no" })
        .unwrap_or("Unavailable");
    let before = row
        .before
        .as_ref()
        .map(|previous| render_safe_option_value(&previous.value));
    let baseline_label = comparison_baseline.as_deref().unwrap_or("Unavailable");
    let change_kind = row.diff.as_ref().map(|diff| match diff.kind {
        crate::api::models::OptionChangeKind::Added => "added",
        crate::api::models::OptionChangeKind::Removed => "removed",
        crate::api::models::OptionChangeKind::Modified => "modified",
        crate::api::models::OptionChangeKind::Unchanged => "unchanged",
    });
    let change_kind_label = change_kind.unwrap_or("changed");
    rsx! {
        div { class: "cfgx-insp-head", div { class: "mono cfgx-insp-path", "config.{option.path}" } }
        dl { class: "cfg-explorer-facts",
            div { dt { "Declared type" } dd { class: "mono", "{declared_type}" } }
            div { dt { "Safe value" } dd { class: "mono", "{value}" } }
            div { dt { "Overridden" } dd { "{overridden}" } }
            div { dt { "Comparison" } dd { if let Some(changed) = row.changed { if changed { "changed" } else { "unchanged" } } else { "Unavailable" } } }
            div { dt { "Baseline" } dd { class: "mono", "{baseline_label}" } }
            if let Some(before) = before { div { dt { "Before" } dd { class: "mono", "{before}" } } }
            div { dt { "After" } dd { class: "mono", "{value}" } }
        }
        if let Some(diff) = row.diff.as_ref() {
            div { class: "cfg-diff",
                h4 { "Typed change" }
                p { class: "mono", "{change_kind_label} · {diff.value_kind}" }
                if !diff.added.is_empty() {
                    div { class: "cfg-diff-add", strong { "Added" } for value in &diff.added { span { class: "mono", "+ {render_typed_diff_value(value)}" } } }
                }
                if !diff.removed.is_empty() {
                    div { class: "cfg-diff-rem", strong { "Removed" } for value in &diff.removed { span { class: "mono", "− {render_typed_diff_value(value)}" } } }
                }
            }
        }
        p { class: "cfg-explorer-copy", "Certified snapshot detail and provenance for this exact target." }
        div { class: "cfg-explorer-provenance",
            h4 { "Certified definitions" }
            if option.definitions.is_empty() {
                p { "Definition provenance is unavailable." }
            }
            for definition in &option.definitions {
                {
                    let source = definition.source_path.as_deref().or(definition.source_input.as_deref()).unwrap_or("Source unavailable");
                    let status = definition.status.as_deref().unwrap_or(if definition.winning { "winning" } else { "overridden" });
                    let definition = definition.clone();
                    rsx! { button { class: if definition.winning { "cfg-def win focus-ring" } else { "cfg-def focus-ring" }, "aria-label": "Inspect definition source {source}", onclick: move |_| on_open_definition.call(definition.clone()), span { class: "mono cfg-def-file", "{source}" } span { class: "cfg-def-note mono", "{status}" } } }
                }
            }
        }
    }
}

#[component]
fn ConfiguredOptions(
    options: Vec<ConfiguredOptionIdentity>,
    details: Signal<HashMap<Vec<String>, ObservationState>>,
    provenance: Signal<HashMap<Vec<String>, ConfigObservationResponse>>,
    on_option: EventHandler<Vec<String>>,
) -> Element {
    rsx! {
        ul { class: "cfg-explorer-configured",
            for option in options {
                {
                    let path = option.path_components.clone();
                    let display = dotted_path(&path);
                    let value = scoped_option_value(details.read().get(&path)).unwrap_or_else(|| "—".into());
                    let source = scoped_option_source(&path, &provenance.read()).unwrap_or_else(|| "—".into());
                    rsx! { li { key: "{option.key}", button { class: "cfgx-row focus-ring", "aria-label": "Inspect configured option {display}", onclick: move |_| on_option.call(path.clone()), span { class: "cfgx-name mono", title: "{display}", "{display}" } span { class: "cfgx-val mono", title: "{value}", "{value}" } span { class: "cfgx-by mono", title: "{source}", "{source}" } } } }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExplorerMode, ObservationState, display_path_parts, dotted_path, initial_scoped_operations,
        merge_tree_observation, next_operation_sequence, observation_error,
        observation_task_is_current, observed_option_paths, should_request_prefix,
        should_start_configured,
    };
    use crate::api::client::ApiClientError;
    use crate::api::models::{
        ConfigObservationChild, ConfigObservationChildKind, ConfigObservationKind,
        ConfigObservationPayload, ConfigObservationResponse,
    };

    fn tree_page(offset: u32, names: &[&str], truncated: bool) -> ConfigObservationResponse {
        ConfigObservationResponse {
            observation_id: uuid::Uuid::new_v4(),
            revision: "a".repeat(40),
            configuration_name: "host".into(),
            schema_version: 1,
            kind: ConfigObservationKind::Prefix,
            path_components: vec!["services".into()],
            child_offset: offset,
            payload: ConfigObservationPayload::Prefix {
                path_components: vec!["services".into()],
                child_offset: offset,
                children: names
                    .iter()
                    .map(|name| ConfigObservationChild {
                        path_components: vec!["services".into(), (*name).into()],
                        key: format!("{name:0<64}"),
                        kind: ConfigObservationChildKind::Option,
                    })
                    .collect(),
                children_truncated: truncated,
                total_children: 3,
            },
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn observation_task_fence_rejects_scope_and_operation_supersession() {
        assert!(observation_task_is_current(4, 4, 8, 8));
        assert!(!observation_task_is_current(4, 5, 8, 8));
        assert!(!observation_task_is_current(4, 4, 8, 9));
    }

    #[test]
    fn configured_index_starts_only_on_first_configured_activation() {
        assert!(!should_start_configured(
            ExplorerMode::Browse,
            &ObservationState::Idle
        ));
        assert!(should_start_configured(
            ExplorerMode::Configured,
            &ObservationState::Idle
        ));
        assert!(!should_start_configured(
            ExplorerMode::Configured,
            &ObservationState::Lifecycle(crate::api::models::ConfigObservationLifecycle::Queued)
        ));
        assert!(!should_start_configured(
            ExplorerMode::Configured,
            &ObservationState::Error("failed".into())
        ));
        assert_eq!(
            usize::from(should_start_configured(
                ExplorerMode::Configured,
                &ObservationState::Idle
            )),
            1
        );
    }

    #[test]
    fn fresh_prefix_expansion_starts_exactly_one_prefix_operation() {
        assert_eq!(usize::from(should_request_prefix(0, false)), 1);
        assert_eq!(usize::from(should_request_prefix(0, true)), 0);
        assert_eq!(usize::from(should_request_prefix(512, true)), 1);
    }

    #[test]
    fn exact_revision_initially_starts_only_one_shallow_root_operation() {
        let operations = initial_scoped_operations(Some(&"a".repeat(40)), true);
        assert_eq!(
            operations
                .iter()
                .filter(|kind| **kind == ConfigObservationKind::Root)
                .count(),
            1
        );
        assert_eq!(
            operations
                .iter()
                .filter(|kind| **kind == ConfigObservationKind::ConfiguredIndex)
                .count(),
            0
        );
        assert_eq!(
            operations
                .iter()
                .filter(|kind| **kind == ConfigObservationKind::Prefix)
                .count(),
            0
        );
        assert_eq!(
            operations
                .iter()
                .filter(|kind| **kind == ConfigObservationKind::Option)
                .count(),
            0
        );
        assert_eq!(
            operations
                .iter()
                .filter(|kind| **kind == ConfigObservationKind::Provenance)
                .count(),
            0
        );
        let full_inventory_requests = 0;
        assert_eq!(full_inventory_requests, 0);
        assert!(initial_scoped_operations(None, true).is_empty());
        assert!(initial_scoped_operations(Some(&"a".repeat(40)), false).is_empty());
    }

    #[test]
    fn each_retry_advances_exactly_one_operation_generation() {
        assert_eq!(next_operation_sequence(7), 8);
        assert_eq!(next_operation_sequence(u64::MAX), u64::MAX);
    }

    #[test]
    fn config_rows_keep_grid_columns_and_left_alignment() {
        let css = include_str!("../../../assets/app.css");
        assert!(css.contains(
            ".cfgx-colhead, .cfgx-row, .cfgx .cfg-explorer-tree-row { display: grid; \
grid-template-columns: minmax(0, 1.3fr) minmax(0, 1fr) 76px;"
        ));
        assert!(css.contains(
            ".cfgx-name, .cfgx-val, .cfgx-by { min-width: 0; overflow: hidden; \
text-align: left;"
        ));
        assert!(css.contains(
            ".cfgx-row:is(:focus, :active), .cfgx .cfg-explorer-tree-row:is(:focus, :active) { text-align: left; }"
        ));
        assert!(!css.contains(
            ".cfgx-by { color: var(--cf-text-muted); font-size: 10px; text-align: right;"
        ));
    }

    #[test]
    fn dotted_paths_are_display_only_and_do_not_define_identity() {
        let nested = vec!["services".to_string(), "api.port".to_string()];
        let flat = vec!["services.api".to_string(), "port".to_string()];
        assert_ne!(nested, flat);
        assert_eq!(dotted_path(&nested), "services.\"api.port\"");
        assert_eq!(dotted_path(&flat), "\"services.api\".port");
        assert_ne!(dotted_path(&nested), dotted_path(&flat));
        assert_eq!(
            display_path_parts(&nested),
            ("services.".into(), "\"api.port\"".into())
        );
        assert_eq!(
            display_path_parts(&flat),
            ("\"services.api\".".into(), "port".into())
        );
    }

    #[test]
    fn partial_search_filters_only_structured_observed_option_paths() {
        let branches = std::collections::HashMap::from([(
            vec!["services".into()],
            ObservationState::Loaded(tree_page(0, &["api.port", "nginx"], false)),
        )]);
        let paths = observed_option_paths(
            &ObservationState::Idle,
            &branches,
            &ObservationState::Idle,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            "api.port",
        );
        assert_eq!(
            paths,
            vec![vec!["services".to_string(), "api.port".to_string()]]
        );
        assert!(
            observed_option_paths(
                &ObservationState::Idle,
                &branches,
                &ObservationState::Idle,
                &std::collections::HashMap::new(),
                &std::collections::HashMap::new(),
                "missing",
            )
            .is_empty()
        );
    }

    #[test]
    fn tree_continuation_merges_exact_next_page_without_duplicates() {
        let first = tree_page(0, &["a", "b"], true);
        let merged = merge_tree_observation(&first, tree_page(2, &["c"], false))
            .expect("exact continuation should merge");
        let ConfigObservationPayload::Prefix {
            children,
            children_truncated,
            ..
        } = merged.payload
        else {
            panic!("merged payload should remain a prefix");
        };
        assert_eq!(
            children
                .iter()
                .map(|child| child.path_components[1].as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert!(!children_truncated);
        assert!(merge_tree_observation(&first, tree_page(1, &["b", "c"], false)).is_err());
        assert!(merge_tree_observation(&first, tree_page(2, &["c", "c"], false)).is_err());
        assert!(merge_tree_observation(&first, tree_page(2, &["d", "c"], false)).is_err());
        assert!(merge_tree_observation(&first, tree_page(2, &[], true)).is_err());
        assert!(merge_tree_observation(&first, tree_page(2, &["c"], true)).is_err());
        assert!(merge_tree_observation(&first, tree_page(2, &["c", "d"], false)).is_err());

        let mut duplicate_key = tree_page(2, &["c"], false);
        let ConfigObservationPayload::Prefix {
            children: first_children,
            ..
        } = &first.payload
        else {
            panic!("fixture should be a prefix page");
        };
        let ConfigObservationPayload::Prefix { children, .. } = &mut duplicate_key.payload else {
            panic!("fixture should be a prefix page");
        };
        children[0].key = first_children[0].key.clone();
        assert!(merge_tree_observation(&first, duplicate_key).is_err());
    }

    #[test]
    fn observation_error_hides_authorization_and_start_failure_details() {
        for error in [
            ApiClientError::Status {
                code: 403,
                body: "protected system exists".into(),
            },
            ApiClientError::Status {
                code: 500,
                body: "internal cache carrier path".into(),
            },
            ApiClientError::Network("private endpoint detail".into()),
        ] {
            let copy = observation_error(&error);
            assert!(!copy.contains("protected system"));
            assert!(!copy.contains("carrier"));
            assert!(!copy.contains("endpoint"));
        }
    }
}
