//! Renders lazy, path-scoped system configuration observations.
//!
//! The Explorer is observational and independent from certified Config
//! snapshots. Structured path components remain the request identity. Dotted
//! paths are presentation only.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use dioxus::prelude::*;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::api::client::{ApiClientError, load_system_config_observation};
use crate::api::models::{
    ConfigObservationChild, ConfigObservationChildKind, ConfigObservationKind,
    ConfigObservationLifecycle, ConfigObservationPayload, ConfigObservationResponse,
    ConfiguredOptionIdentity, CreateConfigObservationRequest, SafeOptionValue,
};
use crate::components::icon::{Icon, IconName};

#[derive(Clone, PartialEq)]
enum ObservationState {
    Idle,
    Lifecycle(ConfigObservationLifecycle),
    Loaded(ConfigObservationResponse),
    Error(String),
}

fn dotted_path(path: &[String]) -> String {
    path.join(".")
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

fn observation_task_is_current(
    expected_scope: u64,
    current_scope: u64,
    expected_operation: u64,
    current_operation: u64,
) -> bool {
    expected_scope == current_scope && expected_operation == current_operation
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
    on_prefix: EventHandler<Vec<String>>,
    on_option: EventHandler<Vec<String>>,
) -> Element {
    rsx! {
        ul { class: "cfg-explorer-tree", role: "group",
            for child in entries {
                {
                    let path = child.path_components.clone();
                    let display = dotted_path(&path);
                    let is_expanded = expanded.read().contains(&path);
                    let branch_state = branches.read().get(&path).cloned().unwrap_or(ObservationState::Idle);
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
                                            move |_| on_prefix.call(path.clone())
                                        },
                                        span { class: if is_expanded { "cfg-caret open" } else { "cfg-caret" }, Icon { name: IconName::ChevronRight, size: 12 } }
                                        span { class: "mono cfg-explorer-path", title: "{display}", "{display}" }
                                        span { class: "cfg-explorer-kind", "prefix" }
                                    }
                                    if is_expanded {
                                        match branch_state.clone() {
                                            ObservationState::Loaded(observation) => match observation.payload {
                                                ConfigObservationPayload::Prefix { children, children_truncated, total_children, .. } => rsx! {
                                                    ExplorerChildren {
                                                        entries: children,
                                                        depth: depth + 1,
                                                        expanded,
                                                        branches,
                                                        on_prefix,
                                                        on_option,
                                                    }
                                                    if children_truncated { div { class: "cfg-explorer-local-note", role: "status", "Showing bounded children; {total_children} exist under {display}." } }
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
                                                            move |event| { event.stop_propagation(); on_prefix.call(path.clone()); }
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
                                        span { class: "cfg-explorer-leaf", Icon { name: IconName::File, size: 12 } }
                                        span { class: "mono cfg-explorer-path", title: "{display}", "{display}" }
                                        span { class: "cfg-explorer-kind", "option" }
                                    }
                                },
                                ConfigObservationChildKind::Unavailable => rsx! {
                                    div { class: "cfg-explorer-tree-row cfg-explorer-unavailable", style: "{row_style}", role: "status", "aria-label": "Unavailable child {display}",
                                        span { class: "cfg-explorer-leaf", Icon { name: IconName::Warn, size: 12 } }
                                        span { class: "mono cfg-explorer-path", title: "{display}", "{display}" }
                                        span { class: "cfg-explorer-kind", "unavailable" }
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
/// The component never uses the certified V2 snapshot token. When `enabled` is
/// false, it displays `disabled_reason` and starts no observation requests.
#[component]
pub(crate) fn ConfigExplorer(
    system_id: Uuid,
    revision: Option<String>,
    enabled: bool,
    disabled_reason: String,
) -> Element {
    let mut scope_sequence = use_signal(|| 0_u64);
    let mut root_sequence = use_signal(|| 0_u64);
    let mut configured_sequence = use_signal(|| 0_u64);
    let mut root_retry = use_signal(|| 0_u64);
    let mut configured_retry = use_signal(|| 0_u64);
    let mut branch_sequence = use_signal(|| 0_u64);
    let mut detail_sequence = use_signal(|| 0_u64);
    let mut provenance_sequence = use_signal(|| 0_u64);
    let mut root = use_signal(|| ObservationState::Idle);
    let mut configured = use_signal(|| ObservationState::Idle);
    let mut branches = use_signal(HashMap::<Vec<String>, ObservationState>::new);
    let mut branch_sequences = use_signal(HashMap::<Vec<String>, u64>::new);
    let mut expanded = use_signal(HashSet::<Vec<String>>::new);
    let mut detail = use_signal(|| ObservationState::Idle);
    let mut detail_path = use_signal(Vec::<String>::new);
    let mut provenance = use_signal(|| ObservationState::Idle);
    let component_active = use_hook(|| Rc::new(Cell::new(true)));
    {
        let component_active = component_active.clone();
        use_drop(move || component_active.set(false));
    }

    {
        use_effect(use_reactive(
            (&revision, &enabled),
            move |(_requested_revision, _enabled)| {
                let sequence = scope_sequence.peek().saturating_add(1);
                scope_sequence.set(sequence);
                branches.set(HashMap::new());
                branch_sequences.set(HashMap::new());
                expanded.set(HashSet::new());
                detail.set(ObservationState::Idle);
                detail_path.set(Vec::new());
                provenance.set(ObservationState::Idle);
            },
        ));
    }

    {
        let component_active = component_active.clone();
        use_effect(use_reactive(
            (&revision, &enabled),
            move |(requested_revision, enabled)| {
                let _retry = *root_retry.read();
                let sequence = root_sequence.peek().saturating_add(1);
                root_sequence.set(sequence);
                let Some(revision) = requested_revision.filter(|_| enabled) else {
                    root.set(ObservationState::Idle);
                    return;
                };
                start_top_level_observation(
                    system_id,
                    revision,
                    ConfigObservationKind::Root,
                    sequence,
                    root_sequence,
                    component_active.clone(),
                    root,
                );
            },
        ));
    }

    {
        let component_active = component_active.clone();
        use_effect(use_reactive(
            (&revision, &enabled),
            move |(requested_revision, enabled)| {
                let _retry = *configured_retry.read();
                let sequence = configured_sequence.peek().saturating_add(1);
                configured_sequence.set(sequence);
                let Some(revision) = requested_revision.filter(|_| enabled) else {
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
            },
        ));
    }

    let load_prefix = EventHandler::new({
        let revision = revision.clone();
        let component_active = component_active.clone();
        move |path: Vec<String>| {
            let currently_expanded = expanded.peek().contains(&path);
            let loaded = matches!(
                branches.peek().get(&path),
                Some(ObservationState::Loaded(_))
            );
            if currently_expanded && loaded {
                expanded.write().remove(&path);
                return;
            }
            expanded.write().insert(path.clone());
            if loaded {
                return;
            }
            let Some(revision) = revision.clone().filter(|_| enabled) else {
                return;
            };
            let sequence = branch_sequence.peek().saturating_add(1);
            branch_sequence.set(sequence);
            let scope = *scope_sequence.peek();
            branch_sequences.write().insert(path.clone(), sequence);
            branches.write().insert(
                path.clone(),
                ObservationState::Lifecycle(ConfigObservationLifecycle::Queued),
            );
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
                    },
                    |request| {
                        if is_current() {
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
                        Ok(Some(observation)) => ObservationState::Loaded(observation),
                        Ok(None) => return,
                        Err(error) => ObservationState::Error(observation_error(&error)),
                    };
                    branches.write().insert(path, next_state);
                }
            });
        }
    });

    let select_option = EventHandler::new({
        let revision = revision.clone();
        let component_active = component_active.clone();
        move |path: Vec<String>| {
            let Some(revision) = revision.clone().filter(|_| enabled) else {
                return;
            };
            let sequence = detail_sequence.peek().saturating_add(1);
            detail_sequence.set(sequence);
            let scope = *scope_sequence.peek();
            detail_path.set(path.clone());
            detail.set(ObservationState::Lifecycle(
                ConfigObservationLifecycle::Queued,
            ));
            provenance.set(ObservationState::Idle);
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
                        Ok(Some(observation)) => ObservationState::Loaded(observation),
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
            let Some(revision) = revision.clone().filter(|_| enabled && !path.is_empty()) else {
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
                        Ok(Some(observation)) => ObservationState::Loaded(observation),
                        Ok(None) => return,
                        Err(error) => ObservationState::Error(observation_error(&error)),
                    });
                }
            });
        }
    };

    let root_state = root.read().clone();
    let configured_state = configured.read().clone();
    let detail_state = detail.read().clone();
    let provenance_state = provenance.read().clone();
    rsx! {
        section { class: "card sd-card cfg-explorer-card",
            div { class: "sd-card-head",
                div { h2 { "Explorer" } p { class: "cfg-explorer-subtitle", "Lazy, scoped observations for this exact revision" } }
                span { class: "sd-card-meta", "observational" }
            }
            if !enabled {
                div { class: "cfg-comparison-note", role: "status", "{disabled_reason}" }
            } else {
                div { class: "cfg-explorer-grid",
                    section { class: "cfg-explorer-pane", "aria-label": "Browse configuration hierarchy",
                        header { class: "cfg-explorer-pane-head", h3 { "Browse hierarchy" } span { "Expands one prefix at a time" } }
                        match root_state.clone() {
                            ObservationState::Loaded(observation) => match observation.payload {
                                ConfigObservationPayload::Root { children, children_truncated, total_children, .. } => rsx! {
                                        ExplorerChildren { entries: children, depth: 0, expanded, branches, on_prefix: load_prefix, on_option: select_option }
                                    if children_truncated { div { class: "cfg-explorer-local-note", role: "status", "Showing bounded top-level children; {total_children} exist." } }
                                },
                                _ => rsx! { div { class: "cfg-explorer-local-error", role: "alert", "Unexpected root observation payload." } },
                            },
                            ObservationState::Error(error) => rsx! {
                                div { class: "cfg-explorer-local-error", role: "alert",
                                    span { "Root: {error}" }
                                    button {
                                        class: "btn btn-ghost focus-ring xs",
                                        "aria-label": "Retry root configuration observation",
                                        onclick: move |_| {
                                            let next = root_retry.peek().saturating_add(1);
                                            root_retry.set(next);
                                        },
                                        "Retry"
                                    }
                                }
                            },
                            state => rsx! { ExplorerStatus { state, surface: "Root" } },
                        }
                    }
                    section { class: "cfg-explorer-pane", "aria-label": "Configured options",
                        header { class: "cfg-explorer-pane-head", h3 { "Configured options" } span { "Surviving assignments" } }
                        p { class: "cfg-explorer-copy", "Identifies surviving non-default module assignments. Declaration defaults are excluded." }
                        match configured_state.clone() {
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
                                    div { class: "cfg-explorer-configured-meta", role: "status", "{total_configured} configured from {total_traversed} traversed" }
                                    if options.is_empty() { div { class: "cfg-explorer-status", "No surviving non-default assignments were observed." } }
                                    else { ConfiguredOptions { options, on_option: select_option } }
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
                                        onclick: move |_| {
                                            let next = configured_retry.peek().saturating_add(1);
                                            configured_retry.set(next);
                                        },
                                        "Retry"
                                    }
                                }
                            },
                            state => rsx! { ExplorerStatus { state, surface: "Configured options" } },
                        }
                    }
                }
                section { class: "cfg-explorer-detail", "aria-label": "Lazy option detail",
                    match detail_state {
                        ObservationState::Idle => rsx! { div { class: "cfg-explorer-detail-empty", "Select an option from either pane to inspect the same lazy detail." } },
                        ObservationState::Loaded(observation) => match observation.payload {
                            ConfigObservationPayload::Option { path_components, declared_type, is_defined, highest_prio, value, .. } => {
                                let path = dotted_path(&path_components);
                                let value_text = render_safe_option_value(&value);
                                let declared_type_label = declared_type.as_deref().unwrap_or("Unavailable");
                                let defined_label = if is_defined { "yes" } else { "no" };
                                let priority_label = highest_prio.map(|value| value.to_string()).unwrap_or_else(|| "Unavailable".into());
                                rsx! {
                                    div { class: "cfg-explorer-detail-head", div { span { class: "cfg-detail-label", "Exact path" } h3 { class: "mono", title: "{path}", "{path}" } } }
                                    dl { class: "cfg-explorer-facts",
                                        div { dt { "Declared type" } dd { class: "mono", "{declared_type_label}" } }
                                        div { dt { "Safe value" } dd { class: if matches!(value, SafeOptionValue::Failed(_)) { "mono cfg-val-err" } else { "mono" }, "{value_text}" } }
                                        div { dt { "Defined" } dd { "{defined_label}" } }
                                        div { dt { "Highest priority" } dd { class: "mono", "{priority_label}" } }
                                    }
                                    p { class: "cfg-explorer-copy", "Basic detail does not imply complete provenance." }
                                    if matches!(provenance_state, ObservationState::Idle | ObservationState::Error(_)) {
                                        button { class: "btn btn-ghost focus-ring xs", "aria-label": "Load provenance for {path}", onclick: load_provenance, "Load provenance" }
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
                    }
                }
            }
        }
    }
}

#[component]
fn ConfiguredOptions(
    options: Vec<ConfiguredOptionIdentity>,
    on_option: EventHandler<Vec<String>>,
) -> Element {
    rsx! {
        ul { class: "cfg-explorer-configured",
            for option in options {
                {
                    let path = option.path_components.clone();
                    let display = dotted_path(&path);
                    rsx! { li { key: "{option.key}", button { class: "cfg-explorer-configured-row focus-ring", "aria-label": "Inspect configured option {display}", onclick: move |_| on_option.call(path.clone()), span { class: "mono", title: "{display}", "{display}" } Icon { name: IconName::ChevronRight, size: 12 } } } }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{observation_error, observation_task_is_current};
    use crate::api::client::ApiClientError;

    #[test]
    fn observation_task_fence_rejects_scope_and_operation_supersession() {
        assert!(observation_task_is_current(4, 4, 8, 8));
        assert!(!observation_task_is_current(4, 5, 8, 8));
        assert!(!observation_task_is_current(4, 4, 8, 9));
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
