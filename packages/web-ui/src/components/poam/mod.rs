//! Reusable POA&M workflow components.
//!
//! Finding identity always comes from server-issued IDs. The presentation
//! fields carried alongside those IDs are read-only context, never identity.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use uuid::Uuid;

use crate::components::icon::{Icon, IconName};
use crate::views::poam_api::{
    self, ActivityView, AddFindingRequest, AddMilestoneRequest, AddNoteRequest, AssessmentOutcome,
    AssignmentReferenceRequest, ClosePreconditionDetails, FindingRelationshipEntry, FindingView,
    MilestoneView, PoamApiError, PoamDetail, PoamDetailQuery, PoamRisk, PoamStatus, PoamSummary,
    RevisionRequest, Rollup, TransitionPoamRequest, UpdateMilestoneRequest, UpdatePoamRequest,
    VerificationResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentVersionCandidate {
    pub assignment_id: Uuid,
    pub assignment_version_id: Uuid,
    pub bundle_id: Uuid,
    pub bundle_version_id: Uuid,
    pub bundle_name: String,
    pub bundle_version: String,
    pub scope_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingPoamContext {
    pub assessment_id: Uuid,
    pub system_id: Uuid,
    pub hostname: String,
    pub policy_lineage_id: Uuid,
    pub policy_version_id: Uuid,
    pub policy_name: String,
    pub policy_version: String,
    pub bundle_id: Option<Uuid>,
    pub bundle_name: Option<String>,
    pub bundle_version_id: Option<Uuid>,
    pub bundle_version: Option<String>,
    pub result: AssessmentOutcome,
    pub evidence_summary: String,
    pub assignment_versions: Vec<AssignmentVersionCandidate>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FindingPoamEvent {
    Open(Uuid),
    Created(PoamDetail),
    Linked(PoamDetail),
    InvalidateAssessment(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PoamFilter {
    #[default]
    Open,
    Overdue,
    Awaiting,
    Closed,
    All,
}

impl PoamFilter {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Overdue => "Overdue",
            Self::Awaiting => "Awaiting verification",
            Self::Closed => "Closed",
            Self::All => "All",
        }
    }

    pub fn includes(self, poam: &PoamSummary) -> bool {
        match self {
            Self::Open => poam.status.is_active(),
            Self::Overdue => poam.status.is_active() && poam.overdue,
            Self::Awaiting => poam.status == PoamStatus::AwaitingVerification,
            Self::Closed => poam.status == PoamStatus::Completed,
            Self::All => true,
        }
    }
}

pub const fn status_label(status: PoamStatus) -> &'static str {
    status.label()
}

pub const fn status_class(status: PoamStatus) -> &'static str {
    match status {
        PoamStatus::Open => "poam-chip-open",
        PoamStatus::InProgress => "poam-chip-progress",
        PoamStatus::Blocked => "poam-chip-blocked",
        PoamStatus::AwaitingVerification => "poam-chip-awaiting",
        PoamStatus::Completed => "poam-chip-completed",
    }
}

pub const fn risk_label(risk: PoamRisk) -> &'static str {
    match risk {
        PoamRisk::High => "CAT I - High",
        PoamRisk::Medium => "CAT II - Medium",
        PoamRisk::Low => "CAT III - Low",
    }
}

pub const fn risk_class(risk: PoamRisk) -> &'static str {
    match risk {
        PoamRisk::High => "poam-risk-high",
        PoamRisk::Medium => "poam-risk-medium",
        PoamRisk::Low => "poam-risk-low",
    }
}

pub const fn result_label(result: VerificationResult) -> &'static str {
    match result {
        VerificationResult::Pass => "Pass",
        VerificationResult::Waiver => "Waiver",
        VerificationResult::Fail => "Fail",
        VerificationResult::Error => "Error",
        VerificationResult::NotChecked => "Not checked",
        VerificationResult::Missing => "Missing",
        VerificationResult::Stale => "Stale",
        VerificationResult::Unknown => "Unknown",
        VerificationResult::Warn => "Warn",
        VerificationResult::NotApplicable => "Not applicable",
    }
}

pub const fn result_class(result: VerificationResult) -> &'static str {
    match result {
        VerificationResult::Pass => "poam-result-pass",
        VerificationResult::Waiver => "poam-result-waiver",
        VerificationResult::Fail | VerificationResult::Error => "poam-result-fail",
        VerificationResult::Warn => "poam-result-warn",
        VerificationResult::NotChecked
        | VerificationResult::Missing
        | VerificationResult::Stale
        | VerificationResult::Unknown
        | VerificationResult::NotApplicable => "poam-result-unknown",
    }
}

fn assessment_label(result: AssessmentOutcome) -> &'static str {
    match result {
        AssessmentOutcome::Pass => "PASS",
        AssessmentOutcome::Fail => "FAIL",
        AssessmentOutcome::Error => "ERROR",
        AssessmentOutcome::NotChecked => "NOT CHECKED",
    }
}

fn format_date(date: Option<NaiveDate>) -> String {
    date.map(|value| value.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "Not set".to_string())
}

fn api_message(error: &PoamApiError) -> String {
    match error {
        PoamApiError::Server(server) => server.message.clone(),
        _ => error.to_string(),
    }
}

#[component]
fn StatusChip(poam: PoamSummary) -> Element {
    rsx! {
        span { class: "poam-chip {status_class(poam.status)}", "{status_label(poam.status)}" }
        if poam.overdue && poam.status.is_active() {
            span { class: "poam-chip poam-chip-overdue", "Overdue" }
        }
    }
}

#[component]
fn RiskChip(risk: PoamRisk) -> Element {
    rsx! { span { class: "poam-chip {risk_class(risk)}", "{risk_label(risk)}" } }
}

#[derive(Props, Clone, PartialEq)]
pub struct FindingPoamBarProps {
    pub context: FindingPoamContext,
    pub relationship: FindingRelationshipEntry,
    #[props(default)]
    pub viewer: bool,
    pub on_event: EventHandler<FindingPoamEvent>,
}

#[component]
pub fn FindingPoamBar(props: FindingPoamBarProps) -> Element {
    let mut create_open = use_signal(|| false);
    let mut link_open = use_signal(|| false);
    let active = props.relationship.active.clone();
    let history = props.relationship.history.clone();
    let can_start = props.context.result == AssessmentOutcome::Fail && active.is_none();

    if props.relationship.assessment_id != props.context.assessment_id {
        return rsx! {
            div { class: "sd-callout sd-callout-danger",
                "Remediation relationship does not match this authoritative assessment. Refresh evidence before continuing."
            }
        };
    }

    if props.context.result != AssessmentOutcome::Fail && active.is_none() && history.is_empty() {
        return rsx! {};
    }

    rsx! {
        section { class: "poam-bar", aria_label: "Finding remediation", "data-testid": "finding-poam-remediation", "data-assessment-id": "{props.context.assessment_id}",
            div { class: "poam-bar-label", Icon { name: IconName::Gear, size: 12 } "Remediation" }
            div { class: "poam-finding-result",
                span { "Current result" }
                strong { class: if props.context.result == AssessmentOutcome::Fail { "poam-current-fail" } else { "" }, "{assessment_label(props.context.result)}" }
            }
            if let Some(poam) = active {
                button { class: "poam-ref focus-ring", onclick: move |_| props.on_event.call(FindingPoamEvent::Open(poam.id)),
                    span { class: "mono poam-human-id", "{poam.human_id}" }
                    StatusChip { poam: poam.clone() }
                    span { class: "poam-muted", "Due {format_date(poam.target_date)}" }
                    Icon { name: IconName::ChevronRight, size: 12 }
                }
            } else if can_start {
                div { class: "poam-bar-actions",
                    button { class: "btn btn-ghost xs focus-ring", disabled: props.viewer, onclick: move |_| create_open.set(true), Icon { name: IconName::Plus, size: 11 } "Create POA&M" }
                    button { class: "btn btn-ghost xs focus-ring", disabled: props.viewer, onclick: move |_| link_open.set(true), Icon { name: IconName::Link, size: 11 } "Link existing" }
                    span { class: "poam-muted", "No active remediation plan. The finding remains FAIL." }
                }
            } else {
                span { class: "poam-muted", "No active remediation plan." }
            }
            if !history.is_empty() {
                div { class: "poam-history",
                    span { "Completed history" }
                    for poam in history {
                        button { key: "{poam.id}", class: "poam-ref poam-ref-quiet focus-ring", onclick: move |_| props.on_event.call(FindingPoamEvent::Open(poam.id)),
                            span { class: "mono", "{poam.human_id}" }
                            span { "closed {format_date(poam.closed_at.map(|value| value.date_naive()))}" }
                        }
                    }
                }
            }
        }
        if create_open() {
            PoamCreateModal {
                context: props.context.clone(),
                on_close: move |_| create_open.set(false),
                on_created: move |detail| {
                    create_open.set(false);
                    props.on_event.call(FindingPoamEvent::Created(detail));
                    props.on_event.call(FindingPoamEvent::InvalidateAssessment(props.context.assessment_id));
                },
            }
        }
        if link_open() {
            PoamLinkExistingModal {
                context: props.context.clone(),
                on_close: move |_| link_open.set(false),
                on_linked: move |detail| {
                    link_open.set(false);
                    props.on_event.call(FindingPoamEvent::Linked(detail));
                    props.on_event.call(FindingPoamEvent::InvalidateAssessment(props.context.assessment_id));
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PoamCreateModalProps {
    context: FindingPoamContext,
    on_close: EventHandler<()>,
    on_created: EventHandler<PoamDetail>,
}

#[component]
fn PoamCreateModal(props: PoamCreateModalProps) -> Element {
    let mut title = use_signal(|| {
        format!(
            "{} remediation on {}",
            props.context.policy_name, props.context.hostname
        )
    });
    let mut owner = use_signal(String::new);
    let mut target = use_signal(String::new);
    let mut risk = use_signal(|| PoamRisk::Medium);
    let mut plan = use_signal(String::new);
    let mut milestones = use_signal(|| true);
    let mut assignments = use_signal(HashSet::<Uuid>::new);
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let close = props.on_close;

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| if !pending() { close.call(()) },
            div { class: "modal poam-modal", role: "dialog", aria_modal: "true", tabindex: "-1", onclick: |event| event.stop_propagation(), onkeydown: move |event| if event.key() == Key::Escape && !pending() { close.call(()) },
                div { class: "modal-head poam-modal-head",
                    div { h2 { "Create POA&M" } p { "Track remediation for a known deficiency. The assessment result does not change." } }
                    button { class: "btn-icon focus-ring", aria_label: "Close", disabled: pending(), onclick: move |_| close.call(()), Icon { name: IconName::X, size: 16 } }
                }
                div { class: "modal-body poam-modal-body",
                    FindingContextPanel { context: props.context.clone() }
                    div { class: "poam-form-grid",
                        label { class: "field poam-span-all", span { "Title" } input { class: "input focus-ring", value: "{title}", disabled: pending(), oninput: move |event| title.set(event.value()) } }
                        label { class: "field", span { "Owner" } input { class: "input focus-ring", value: "{owner}", placeholder: "Responsible team or person", disabled: pending(), oninput: move |event| owner.set(event.value()) } }
                        label { class: "field", span { "Target completion" } input { class: "input focus-ring mono", r#type: "date", value: "{target}", disabled: pending(), oninput: move |event| target.set(event.value()) } }
                        label { class: "field", span { "Risk" } select { class: "input focus-ring", value: "{risk:?}", disabled: pending(), onchange: move |event| risk.set(match event.value().as_str() { "High" => PoamRisk::High, "Low" => PoamRisk::Low, _ => PoamRisk::Medium }), option { value: "High", "CAT I - High" } option { value: "Medium", "CAT II - Medium" } option { value: "Low", "CAT III - Low" } } }
                        label { class: "field poam-span-all", span { "Remediation plan" } textarea { class: "input focus-ring", rows: "4", value: "{plan}", placeholder: "What will change, where, and how it will be verified", disabled: pending(), oninput: move |event| plan.set(event.value()) } }
                    }
                    label { class: "poam-check", input { r#type: "checkbox", checked: milestones(), disabled: pending(), onchange: move |event| milestones.set(event.checked()) } span { "Start with server-standard milestones" small { " Server dates the standard module, staging, validation, production, and verification milestones." } } }
                    if !props.context.assignment_versions.is_empty() {
                        section { class: "poam-subsection",
                            h3 { "Baseline assignment reference" }
                            p { "Supplemental reference only. Linking an exact immutable assignment version does not alter that assignment." }
                            for candidate in props.context.assignment_versions.clone() {
                                label { class: "poam-check poam-assignment-choice",
                                    input { r#type: "checkbox", checked: assignments.read().contains(&candidate.assignment_version_id), disabled: pending(), onchange: move |event| { let mut next = assignments.read().clone(); if event.checked() { next.insert(candidate.assignment_version_id); } else { next.remove(&candidate.assignment_version_id); } assignments.set(next); } }
                                    span { strong { "{candidate.bundle_name} {candidate.bundle_version}" } small { class: "mono", "{candidate.scope_label} · version {candidate.assignment_version_id}" } }
                                }
                            }
                        }
                    }
                    div { class: "sd-callout sd-callout-info", Icon { name: IconName::Shield, size: 13 } div { "A POA&M records work to fix a deficiency. Risk acceptance uses the separate waiver flow; neither action changes this finding's result." } }
                    if let Some(message) = error() { div { class: "sd-callout sd-callout-danger", Icon { name: IconName::Warn, size: 13 } div { "{message}" } } }
                }
                div { class: "modal-foot",
                    button { class: "btn btn-ghost focus-ring", disabled: pending(), onclick: move |_| close.call(()), "Cancel" }
                    button { class: "btn btn-primary focus-ring", disabled: pending() || title.read().trim().is_empty() || owner.read().trim().is_empty(), onclick: move |_| {
                        let parsed_target = if target.read().trim().is_empty() { Ok(None) } else { NaiveDate::parse_from_str(target.read().trim(), "%Y-%m-%d").map(Some).map_err(|_| "Enter a valid target date.".to_string()) };
                        let Ok(target_date) = parsed_target else { error.set(parsed_target.err()); return; };
                        let request = poam_api::CreatePoamRequest { assessment_id: props.context.assessment_id, title: title.read().trim().to_string(), plan: plan.read().trim().to_string(), owner: owner.read().trim().to_string(), target_date, risk: risk(), default_milestones: milestones(), assignment_version_ids: assignments.read().iter().copied().collect() };
                        let mut pending = pending; let mut error = error; let on_created = props.on_created;
                        spawn(async move { pending.set(true); match poam_api::create_poam(&request).await { Ok(detail) => on_created.call(detail), Err(err) => { error.set(Some(if err.is_active_remediation() { "This finding already has an active remediation plan. Refresh the finding before retrying.".to_string() } else { api_message(&err) })); pending.set(false); } } });
                    }, if pending() { "Creating..." } else { "Create POA&M" } }
                }
            }
        }
    }
}

#[component]
fn FindingContextPanel(context: FindingPoamContext) -> Element {
    let bundle = match (&context.bundle_name, &context.bundle_version) {
        (Some(name), Some(version)) => format!("{name} · {version}"),
        (Some(name), None) => name.clone(),
        _ => "No bundle context".to_string(),
    };
    rsx! {
        section { class: "poam-context", "data-testid": "poam-finding-context",
            header { Icon { name: IconName::Shield, size: 12 } "Finding context" span { "Authoritative and read-only" } }
            dl {
                div { dt { "System" } dd { class: "mono", "{context.hostname}" } }
                div { dt { "Policy / version" } dd { "{context.policy_name} · {context.policy_version}" } }
                div { dt { "Bundle / version" } dd { "{bundle}" } }
                div { dt { "Result" } dd { class: "poam-current-fail", "{assessment_label(context.result)}" } }
                div { dt { "Assessment" } dd { class: "mono", "{context.assessment_id}" } }
                div { dt { "Evidence" } dd { "{context.evidence_summary}" } }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PoamLinkExistingModalProps {
    context: FindingPoamContext,
    on_close: EventHandler<()>,
    on_linked: EventHandler<PoamDetail>,
}

#[component]
fn PoamLinkExistingModal(props: PoamLinkExistingModalProps) -> Element {
    let mut query = use_signal(String::new);
    let mut generation = use_signal(|| 0_u64);
    let mut results = use_signal(Vec::<PoamSummary>::new);
    let mut loading = use_signal(|| true);
    let mut pending = use_signal(|| None::<Uuid>);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_nonce = use_signal(|| 0_u64);
    let close = props.on_close;

    use_effect(move || {
        let term = query();
        let _ = refresh_nonce();
        generation += 1;
        let request_generation = generation();
        spawn(async move {
            loading.set(true);
            TimeoutFuture::new(300).await;
            if generation() != request_generation {
                return;
            }
            match poam_api::compatible_poams(
                props.context.assessment_id,
                (!term.trim().is_empty()).then_some(term.trim()),
                50,
                0,
            )
            .await
            {
                Ok(page) if generation() == request_generation => {
                    results.set(page.items);
                    error.set(None);
                    loading.set(false);
                }
                Err(err) if generation() == request_generation => {
                    results.set(Vec::new());
                    error.set(Some(api_message(&err)));
                    loading.set(false);
                }
                _ => {}
            }
        });
    });

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| if pending().is_none() { close.call(()) },
            div { class: "modal poam-modal poam-link-modal", role: "dialog", aria_modal: "true", tabindex: "-1", onclick: |event| event.stop_propagation(), onkeydown: move |event| if event.key() == Key::Escape && pending().is_none() { close.call(()) },
                div { class: "modal-head poam-modal-head",
                    div { h2 { "Link existing POA&M" } p { "Only server-confirmed compatible remediation plans are shown for {props.context.hostname} / {props.context.policy_name}." } }
                    button { class: "btn-icon focus-ring", aria_label: "Close", disabled: pending().is_some(), onclick: move |_| close.call(()), Icon { name: IconName::X, size: 16 } }
                }
                div { class: "modal-body poam-modal-body",
                    div { class: "filter-search poam-search", Icon { name: IconName::Search, size: 12 } input { class: "input focus-ring", autofocus: true, value: "{query}", placeholder: "Search by POA&M ID, title, or owner", disabled: pending().is_some(), oninput: move |event| query.set(event.value()) } }
                    p { class: "poam-muted", "Compatibility and the one-active-remediation rule are enforced by the server. Linking does not change the FAIL result." }
                    if loading() { div { class: "poam-empty", "Searching compatible POA&M items..." } }
                    if let Some(message) = error() { div { class: "sd-callout sd-callout-danger", "{message}" } }
                    if !loading() && error().is_none() && results.read().is_empty() { div { class: "poam-empty", "No compatible active POA&M items match." } }
                    div { class: "poam-picker-list",
                        for item in results.read().clone() {
                            button { key: "{item.id}", class: "poam-pick focus-ring", disabled: pending().is_some(), onclick: move |_| {
                                pending.set(Some(item.id)); error.set(None);
                                let request = AddFindingRequest { revision: item.revision, assessment_id: props.context.assessment_id };
                                let mut pending = pending; let mut error = error; let on_linked = props.on_linked;
                                spawn(async move { match poam_api::link_poam_finding(item.id, &request).await { Ok(detail) => on_linked.call(detail), Err(err) => { let stale = err.is_stale(); error.set(Some(if err.is_active_remediation() { "This finding already has an active remediation plan. Refresh before retrying.".to_string() } else if stale { "This POA&M changed while it was selected. Refreshed the current server results; retry the updated item.".to_string() } else { api_message(&err) })); pending.set(None); if stale { refresh_nonce += 1; } } } });
                            },
                                div { class: "poam-pick-head", span { class: "mono poam-human-id", "{item.human_id}" } StatusChip { poam: item.clone() } RiskChip { risk: item.risk } }
                                strong { "{item.title}" }
                                small { "{item.owner} · due {format_date(item.target_date)} · {item.finding_count} linked findings · revision {item.revision}" }
                                if pending() == Some(item.id) { span { class: "poam-pending", "Linking..." } }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct PoamDetailHostProps {
    pub poam_id: Option<Uuid>,
    #[props(default)]
    pub viewer: bool,
    #[props(default)]
    pub assignment_versions: Vec<AssignmentVersionCandidate>,
    pub on_close: EventHandler<()>,
    pub on_open_finding: EventHandler<FindingView>,
    #[props(default)]
    pub on_changed: Option<EventHandler<PoamDetail>>,
}

#[component]
pub fn PoamDetailHost(props: PoamDetailHostProps) -> Element {
    let Some(poam_id) = props.poam_id else {
        return rsx! {};
    };
    rsx! { PoamDetailTray { key: "{poam_id}", poam_id, viewer: props.viewer, assignment_versions: props.assignment_versions, on_close: props.on_close, on_open_finding: props.on_open_finding, on_changed: props.on_changed } }
}

#[derive(Props, Clone, PartialEq)]
pub struct PoamDetailTrayProps {
    pub poam_id: Uuid,
    #[props(default)]
    pub viewer: bool,
    #[props(default)]
    pub assignment_versions: Vec<AssignmentVersionCandidate>,
    pub on_close: EventHandler<()>,
    pub on_open_finding: EventHandler<FindingView>,
    #[props(default)]
    pub on_changed: Option<EventHandler<PoamDetail>>,
}

#[derive(Debug, Clone, PartialEq)]
enum DetailState {
    Loading,
    NotVisible,
    Failed(String),
    Loaded(PoamDetail),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MilestoneDraft {
    title: String,
    target: String,
}

#[component]
pub fn PoamDetailTray(props: PoamDetailTrayProps) -> Element {
    let mut state = use_signal(|| DetailState::Loading);
    let mut generation = use_signal(|| 0_u64);
    let mut busy = use_signal(|| None::<String>);
    let mut message = use_signal(|| None::<String>);
    let mut close_details = use_signal(|| None::<ClosePreconditionDetails>);
    let mut title = use_signal(String::new);
    let mut owner = use_signal(String::new);
    let mut target = use_signal(String::new);
    let mut risk = use_signal(|| PoamRisk::Medium);
    let mut plan = use_signal(String::new);
    let mut note = use_signal(String::new);
    let mut milestone_title = use_signal(String::new);
    let mut milestone_target = use_signal(String::new);
    let mut milestone_drafts = use_signal(HashMap::<Uuid, MilestoneDraft>::new);
    let mut loaded_poam_id = use_signal(|| None::<Uuid>);
    let mut finding_picker = use_signal(|| false);
    let mut finding_query = use_signal(String::new);
    let mut finding_generation = use_signal(|| 0_u64);
    let mut finding_results = use_signal(Vec::new);
    let mut finding_loading = use_signal(|| false);
    let mut assignment_choice = use_signal(String::new);
    let close = props.on_close;

    let mut load = move |reset_drafts: bool| {
        generation += 1;
        let requested = generation();
        state.set(DetailState::Loading);
        spawn(async move {
            match poam_api::fetch_poam(props.poam_id, &PoamDetailQuery::default()).await {
                Ok(detail) if generation() == requested => {
                    if reset_drafts {
                        title.set(detail.poam.title.clone());
                        owner.set(detail.poam.owner.clone());
                        target.set(
                            detail
                                .poam
                                .target_date
                                .map(|date| date.to_string())
                                .unwrap_or_default(),
                        );
                        risk.set(detail.poam.risk);
                        plan.set(detail.poam.plan.clone());
                        milestone_drafts.set(
                            detail
                                .milestones
                                .iter()
                                .map(|item| {
                                    (
                                        item.id,
                                        MilestoneDraft {
                                            title: item.title.clone(),
                                            target: item.target_date.to_string(),
                                        },
                                    )
                                })
                                .collect(),
                        );
                    }
                    state.set(DetailState::Loaded(detail));
                }
                Err(err)
                    if generation() == requested
                        && (err.is_not_visible() || err.is_unauthorized()) =>
                {
                    state.set(DetailState::NotVisible)
                }
                Err(err) if generation() == requested => {
                    state.set(DetailState::Failed(api_message(&err)))
                }
                _ => {}
            }
        });
    };

    use_effect(move || {
        if loaded_poam_id() == Some(props.poam_id) {
            return;
        }
        loaded_poam_id.set(Some(props.poam_id));
        load(true);
    });

    use_effect(move || {
        if !finding_picker() {
            return;
        }
        let term = finding_query();
        finding_generation += 1;
        let requested = finding_generation();
        spawn(async move {
            finding_loading.set(true);
            TimeoutFuture::new(300).await;
            if finding_generation() != requested {
                return;
            }
            match poam_api::compatible_findings(
                props.poam_id,
                (!term.trim().is_empty()).then_some(term.trim()),
                50,
                0,
            )
            .await
            {
                Ok(page) if finding_generation() == requested => {
                    finding_results.set(page.items);
                    finding_loading.set(false);
                }
                Err(err) if finding_generation() == requested => {
                    message.set(Some(api_message(&err)));
                    finding_results.set(Vec::new());
                    finding_loading.set(false);
                }
                _ => {}
            }
        });
    });

    let detail = match state.read().clone() {
        DetailState::Loading => {
            return tray_shell(
                props.on_close,
                rsx! { div { class: "poam-tray-state", "Loading POA&M..." } },
            );
        }
        DetailState::NotVisible => {
            return tray_shell(
                props.on_close,
                rsx! { div { class: "poam-tray-state", h2 { "POA&M not visible" } p { "It may not exist or you may not have access to its scope." } } },
            );
        }
        DetailState::Failed(error) => {
            return tray_shell(
                props.on_close,
                rsx! { div { class: "poam-tray-state", h2 { "Could not load POA&M" } p { "{error}" } button { class: "btn btn-ghost focus-ring", onclick: move |_| load(true), "Retry" } } },
            );
        }
        DetailState::Loaded(detail) => detail,
    };
    let revision = detail.poam.revision;
    let readonly = props.viewer || busy().is_some();
    let linked_assignment_ids = detail
        .assignment_references
        .iter()
        .map(|item| item.assignment_version_id)
        .collect::<HashSet<_>>();
    let assignment_candidates = props
        .assignment_versions
        .iter()
        .filter(|item| !linked_assignment_ids.contains(&item.assignment_version_id))
        .cloned()
        .collect::<Vec<_>>();
    let completed_milestones = detail
        .milestones
        .iter()
        .filter(|item| item.completed_at.is_some())
        .count();
    let progress = if detail.milestones.is_empty() {
        0
    } else {
        completed_milestones * 100 / detail.milestones.len()
    };

    let mut reconcile = move |next: PoamDetail| {
        if let Some(handler) = props.on_changed {
            handler.call(next.clone());
        }
        let milestone_ids = next
            .milestones
            .iter()
            .map(|milestone| milestone.id)
            .collect::<HashSet<_>>();
        let mut next_drafts = milestone_drafts.read().clone();
        next_drafts.retain(|id, _| milestone_ids.contains(id));
        for milestone in &next.milestones {
            next_drafts
                .entry(milestone.id)
                .or_insert_with(|| MilestoneDraft {
                    title: milestone.title.clone(),
                    target: milestone.target_date.to_string(),
                });
        }
        milestone_drafts.set(next_drafts);
        state.set(DetailState::Loaded(next));
        busy.set(None);
        message.set(None);
        close_details.set(None);
    };
    let mut handle_error = move |intent: &'static str, err: PoamApiError| {
        busy.set(None);
        if err.is_stale() {
            message.set(Some(format!("This POA&M changed before {intent}. Your entered values are preserved; review the current revision and retry.")));
            load(false);
        } else if err.is_active_remediation() {
            message.set(Some(
                "That finding is already linked to another active remediation plan.".to_string(),
            ));
        } else {
            if let Some(details) = err.close_precondition_details() {
                close_details.set(Some(details));
            }
            message.set(Some(api_message(&err)));
        }
    };

    rsx! {
        div { class: "poam-tray-backdrop", onclick: move |_| if busy().is_none() { close.call(()) } }
        aside { class: "poam-tray", role: "dialog", aria_modal: "true", tabindex: "-1", "data-testid": "poam-detail", "data-poam-id": "{detail.poam.id}", "data-poam-revision": "{detail.poam.revision}", onkeydown: move |event| if event.key() == Key::Escape && busy().is_none() { close.call(()) },
            header { class: "poam-tray-head",
                div { class: "poam-tray-title", Icon { name: IconName::Gear, size: 18 } div { div { class: "poam-title-line", span { class: "mono poam-human-id", "{detail.poam.human_id}" } StatusChip { poam: detail.poam.clone() } RiskChip { risk: detail.poam.risk } } p { "{detail.poam.title}" } } }
                button { class: "btn-icon focus-ring", aria_label: "Close", disabled: busy().is_some(), onclick: move |_| close.call(()), Icon { name: IconName::X, size: 16 } }
            }
            div { class: "poam-tray-scroll",
                if let Some(text) = message() { div { class: "poam-tray-alert sd-callout sd-callout-warn", Icon { name: IconName::Warn, size: 13 } div { "{text}" } } }
                section { class: "poam-meta-grid",
                    div { span { "Owner" } strong { "{detail.poam.owner}" } }
                    div { span { "Target" } strong { class: if detail.poam.overdue { "poam-overdue" } else { "" }, "{format_date(detail.poam.target_date)}" } }
                    div { span { "Opened" } strong { class: "mono", "{detail.poam.created_at.date_naive()}" } }
                    div { span { "Milestones" } strong { class: "mono", "{completed_milestones}/{detail.milestones.len()}" } div { class: "poam-progress", span { style: "width:{progress}%" } } }
                }
                section { class: "poam-tray-section",
                    header { h3 { "Metadata" } button { class: "btn btn-ghost xs focus-ring", disabled: readonly, onclick: move |_| {
                        let target_date = if target.read().trim().is_empty() { Ok(None) } else { NaiveDate::parse_from_str(target.read().trim(), "%Y-%m-%d").map(Some).map_err(|_| ()) };
                        let Ok(target_date) = target_date else { message.set(Some("Enter a valid target date.".to_string())); return; };
                        let request = UpdatePoamRequest { revision, title: Some(title.read().trim().to_string()), plan: Some(plan.read().to_string()), owner: Some(owner.read().trim().to_string()), target_date: Some(target_date), risk: Some(risk()) };
                        busy.set(Some("Saving metadata".to_string())); spawn(async move { match poam_api::update_poam(props.poam_id, &request).await { Ok(next) => reconcile(next), Err(err) => handle_error("saving metadata", err) } });
                    }, if busy().is_some() { "Working..." } else { "Save metadata" } } }
                    div { class: "poam-form-grid",
                        label { class: "field poam-span-all", span { "Title" } input { class: "input focus-ring", value: "{title}", disabled: readonly, oninput: move |event| title.set(event.value()) } }
                        label { class: "field", span { "Owner" } input { class: "input focus-ring", value: "{owner}", disabled: readonly, oninput: move |event| owner.set(event.value()) } }
                        label { class: "field", span { "Target completion" } input { class: "input focus-ring mono", r#type: "date", value: "{target}", disabled: readonly, oninput: move |event| target.set(event.value()) } }
                        label { class: "field", span { "Risk" } select { class: "input focus-ring", value: "{risk:?}", disabled: readonly, onchange: move |event| risk.set(match event.value().as_str() { "High" => PoamRisk::High, "Low" => PoamRisk::Low, _ => PoamRisk::Medium }), option { value: "High", "CAT I - High" } option { value: "Medium", "CAT II - Medium" } option { value: "Low", "CAT III - Low" } } }
                    }
                }
                LifecycleSection { detail: detail.clone(), readonly, close_details: close_details(), on_transition: move |status| { let request = TransitionPoamRequest { revision, status, note: None }; busy.set(Some("Changing status".to_string())); spawn(async move { match poam_api::transition_poam(props.poam_id, &request).await { Ok(next) => reconcile(next), Err(err) => handle_error("changing status", err) } }); }, on_verify: move |_| { let request = RevisionRequest { revision }; busy.set(Some("Verifying".to_string())); spawn(async move { match poam_api::verify_poam(props.poam_id, &request).await { Ok(_) => { busy.set(None); load(true); }, Err(err) => handle_error("verifying remediation", err) } }); }, on_close: move |_| { let request = RevisionRequest { revision }; busy.set(Some("Closing".to_string())); spawn(async move { match poam_api::close_poam(props.poam_id, &request).await { Ok(next) => reconcile(next), Err(err) => handle_error("closing", err) } }); }, on_reopen: move |_| { let request = RevisionRequest { revision }; busy.set(Some("Reopening".to_string())); spawn(async move { match poam_api::reopen_poam(props.poam_id, &request).await { Ok(next) => reconcile(next), Err(err) => handle_error("reopening", err) } }); } }
                section { class: "poam-tray-section",
                    header { h3 { "Linked findings · {detail.findings.len()}" } button { class: "btn btn-ghost xs focus-ring", disabled: readonly, onclick: move |_| finding_picker.toggle(), Icon { name: IconName::Link, size: 11 } "Link finding" } }
                    if detail.findings.is_empty() { div { class: "poam-empty", "No findings are linked." } }
                    div { class: "poam-table-wrap", table { class: "sys-table compact sys-table-dense poam-findings-table", thead { tr { th { "Host" } th { "Policy" } th { "Current result" } th { "Actions" } } } tbody {
                        for finding in detail.findings.clone() { { let finding_for_navigation = finding.clone(); let finding_id = finding.id; rsx! { tr { key: "{finding.link_id}", "data-testid": "poam-linked-finding", "data-finding-id": "{finding.id}", td { class: "mono", "{finding.hostname}" } td { "{finding.policy_name}" } td { span { class: "poam-chip {result_class(finding.resolution_state)}", "{result_label(finding.resolution_state)}" } } td { class: "poam-row-actions", button { class: "btn btn-ghost xs focus-ring", onclick: move |_| props.on_open_finding.call(finding_for_navigation.clone()), "Evidence" } button { class: "btn-icon focus-ring", title: "Unlink finding", disabled: readonly, onclick: move |_| { busy.set(Some("Unlinking finding".to_string())); spawn(async move { match poam_api::unlink_poam_finding(props.poam_id, finding_id, revision).await { Ok(next) => reconcile(next), Err(err) => handle_error("unlinking finding", err) } }); }, Icon { name: IconName::X, size: 12 } } } } } } }
                    } } }
                    if finding_picker() {
                        div { class: "poam-picker-panel",
                            div { class: "filter-search poam-search",
                                Icon { name: IconName::Search, size: 12 }
                                input { class: "input focus-ring", value: "{finding_query}", placeholder: "Search compatible failing findings", oninput: move |event| finding_query.set(event.value()) }
                            }
                            if finding_loading() { p { class: "poam-muted", "Searching..." } }
                            for candidate in finding_results.read().clone() {
                                {
                                    let outcome = candidate.outcome.map(assessment_label).unwrap_or("UNKNOWN");
                                    rsx! {
                                        button { class: "poam-pick focus-ring", disabled: readonly,
                                            onclick: move |_| {
                                                let Some(assessment_id) = candidate.assessment_id else {
                                                    message.set(Some("This candidate has no current authoritative assessment.".to_string()));
                                                    return;
                                                };
                                                let request = AddFindingRequest { revision, assessment_id };
                                                busy.set(Some("Linking finding".to_string()));
                                                spawn(async move {
                                                    match poam_api::link_poam_finding(props.poam_id, &request).await {
                                                        Ok(next) => { finding_picker.set(false); reconcile(next); }
                                                        Err(err) => handle_error("linking finding", err),
                                                    }
                                                });
                                            },
                                            strong { class: "mono", "{candidate.hostname}" }
                                            span { "{candidate.policy_name}" }
                                            small { "Current result: {outcome}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                section { class: "poam-tray-section",
                    header { h3 { "Baseline assignment references · {detail.assignment_references.len()}" } }
                    p { class: "poam-section-help", "Supplemental references to exact immutable assignment versions. These links never change assignment content or current-version pointers." }
                    for reference in detail.assignment_references.clone() { div { class: "poam-assignment-row", div { strong { "{reference.bundle_name} {reference.bundle_version}" } small { class: "mono", "Assignment version {reference.assignment_version_id}" } } button { class: "btn btn-ghost xs focus-ring", disabled: readonly, onclick: move |_| { busy.set(Some("Unlinking assignment reference".to_string())); spawn(async move { match poam_api::unlink_poam_assignment(props.poam_id, reference.assignment_version_id, revision).await { Ok(next) => reconcile(next), Err(err) => handle_error("unlinking assignment reference", err) } }); }, "Unlink reference" } } }
                    if !assignment_candidates.is_empty() { div { class: "poam-assignment-link", select { class: "input focus-ring", value: "{assignment_choice}", disabled: readonly, onchange: move |event| assignment_choice.set(event.value()), option { value: "", "Select exact assignment version" } for candidate in assignment_candidates.clone() { option { value: "{candidate.assignment_version_id}", "{candidate.bundle_name} {candidate.bundle_version} · {candidate.scope_label}" } } } button { class: "btn btn-ghost focus-ring", disabled: readonly || assignment_choice.read().is_empty(), onclick: move |_| { let Ok(assignment_version_id) = Uuid::parse_str(assignment_choice.read().as_str()) else { return; }; let request = AssignmentReferenceRequest { revision, assignment_version_id }; busy.set(Some("Linking assignment reference".to_string())); spawn(async move { match poam_api::link_poam_assignment(props.poam_id, &request).await { Ok(next) => { assignment_choice.set(String::new()); reconcile(next); }, Err(err) => handle_error("linking assignment reference", err) } }); }, "Link reference" } } }
                }
                section { class: "poam-tray-section", header { h3 { "Remediation plan" } } textarea { class: "input focus-ring poam-plan", rows: "5", value: "{plan}", disabled: readonly, placeholder: "What will change, where, and how it will be verified", oninput: move |event| plan.set(event.value()) } }
                MilestonesSection { milestones: detail.milestones.clone(), drafts: milestone_drafts, new_title: milestone_title, new_target: milestone_target, readonly, on_add: move |values: (String, String)| { let (new_title, new_target) = values; let Ok(target_date) = NaiveDate::parse_from_str(&new_target, "%Y-%m-%d") else { message.set(Some("Enter a valid milestone target date.".to_string())); return; }; let request = AddMilestoneRequest { revision, title: new_title, target_date }; busy.set(Some("Adding milestone".to_string())); spawn(async move { match poam_api::add_poam_milestone(props.poam_id, &request).await { Ok(next) => { milestone_title.set(String::new()); milestone_target.set(String::new()); reconcile(next); }, Err(err) => handle_error("adding milestone", err) } }); }, on_update: move |values: (Uuid, MilestoneDraft, Option<bool>)| { let (id, draft, completed) = values; let Ok(target_date) = NaiveDate::parse_from_str(&draft.target, "%Y-%m-%d") else { message.set(Some("Enter a valid milestone target date.".to_string())); return; }; let request = UpdateMilestoneRequest { revision, title: Some(draft.title), target_date: Some(target_date), completed }; busy.set(Some("Updating milestone".to_string())); spawn(async move { match poam_api::update_poam_milestone(props.poam_id, id, &request).await { Ok(next) => reconcile(next), Err(err) => handle_error("updating milestone", err) } }); }, on_remove: move |id| { busy.set(Some("Removing milestone".to_string())); spawn(async move { match poam_api::remove_poam_milestone(props.poam_id, id, revision).await { Ok(next) => reconcile(next), Err(err) => handle_error("removing milestone", err) } }); } }
                section { class: "poam-tray-section", header { h3 { "Activity" } } ActivityList { activity: detail.activity.clone() } div { class: "poam-note-form", input { class: "input focus-ring", value: "{note}", placeholder: "Add a durable note", disabled: readonly, oninput: move |event| note.set(event.value()) } button { class: "btn btn-ghost focus-ring", disabled: readonly || note.read().trim().is_empty(), onclick: move |_| { let request = AddNoteRequest { revision, text: note.read().trim().to_string() }; busy.set(Some("Adding note".to_string())); spawn(async move { match poam_api::add_poam_note(props.poam_id, &request).await { Ok(next) => { note.set(String::new()); reconcile(next); }, Err(err) => handle_error("adding note", err) } }); }, "Add note" } } }
            }
        }
    }
}

fn tray_shell(on_close: EventHandler<()>, content: Element) -> Element {
    rsx! { div { class: "poam-tray-backdrop", onclick: move |_| on_close.call(()) } aside { class: "poam-tray", role: "dialog", aria_modal: "true", header { class: "poam-tray-head", strong { "POA&M" } button { class: "btn-icon focus-ring", onclick: move |_| on_close.call(()), Icon { name: IconName::X, size: 16 } } } {content} } }
}

#[derive(Props, Clone, PartialEq)]
struct LifecycleSectionProps {
    detail: PoamDetail,
    readonly: bool,
    close_details: Option<ClosePreconditionDetails>,
    on_transition: EventHandler<PoamStatus>,
    on_verify: EventHandler<()>,
    on_close: EventHandler<()>,
    on_reopen: EventHandler<()>,
}

#[component]
fn LifecycleSection(props: LifecycleSectionProps) -> Element {
    let status = props.detail.poam.status;
    let latest_attempt = props.detail.verification_attempts.first();
    rsx! {
        section { class: "poam-tray-section",
            header { h3 { "Remediation status" } div { class: "poam-lifecycle-actions", if status == PoamStatus::Completed { button { class: "btn btn-ghost xs focus-ring", disabled: props.readonly, onclick: move |_| props.on_reopen.call(()), Icon { name: IconName::Rollback, size: 11 } "Reopen" } } else { button { class: "btn btn-ghost xs focus-ring", disabled: props.readonly, onclick: move |_| props.on_verify.call(()), "Verify now" } if status == PoamStatus::AwaitingVerification { button { class: "btn btn-primary xs focus-ring", disabled: props.readonly, onclick: move |_| props.on_close.call(()), Icon { name: IconName::Check, size: 11 } "Authoritative close" } } } } }
            if status != PoamStatus::Completed { div { class: "seg poam-status-seg", for choice in [PoamStatus::Open, PoamStatus::InProgress, PoamStatus::Blocked, PoamStatus::AwaitingVerification] { button { class: if status == choice { "active" } else { "" }, disabled: props.readonly || status == choice, onclick: move |_| props.on_transition.call(choice), "{status_label(choice)}" } } } }
            if status == PoamStatus::AwaitingVerification { div { class: "sd-callout sd-callout-warn", Icon { name: IconName::Warn, size: 13 } div { strong { "Awaiting verification." } " Remediation is reported complete, but the finding result remains independent. Verify against current assessments, then use authoritative close." } } }
            if let Some(attempt) = latest_attempt { div { class: "poam-verification", "data-testid": "poam-verification-result", strong { "Latest verification · {attempt.outcome:?}" } span { class: "mono", "{attempt.attempted_at}" } for item in attempt.items.clone() { div { class: "poam-verification-item", "data-finding-id": "{item.finding_id}", span { class: "mono", "{item.finding_id}" } span { class: "poam-chip {result_class(item.result)}", "{result_label(item.result)}" } span { "{item.detail}" } if item.result == VerificationResult::Waiver { strong { "Closure basis: waiver" } } } } } }
            if let Some(details) = props.close_details { div { class: "poam-close-details", "data-testid": "poam-close-rejection", strong { "Closure rejected for these findings" } for item in details.items { div { "data-finding-id": "{item.finding_id}", span { class: "mono", "{item.finding_id}" } span { class: "poam-chip {result_class(item.result)}", "{result_label(item.result)}" } if item.result == VerificationResult::Waiver { span { "Accepted basis: waiver" } } } } } }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct MilestonesSectionProps {
    milestones: Vec<MilestoneView>,
    drafts: Signal<HashMap<Uuid, MilestoneDraft>>,
    new_title: Signal<String>,
    new_target: Signal<String>,
    readonly: bool,
    on_add: EventHandler<(String, String)>,
    on_update: EventHandler<(Uuid, MilestoneDraft, Option<bool>)>,
    on_remove: EventHandler<Uuid>,
}

#[component]
fn MilestonesSection(props: MilestonesSectionProps) -> Element {
    let mut drafts = props.drafts;
    let mut new_title = props.new_title;
    let mut new_target = props.new_target;
    rsx! { section { class: "poam-tray-section", header { h3 { "Milestones · {props.milestones.iter().filter(|item| item.completed_at.is_some()).count()} of {props.milestones.len()} complete" } }
        div { class: "poam-milestones", for milestone in props.milestones.clone() { if let Some(draft) = drafts.read().get(&milestone.id).cloned() { div { class: "poam-milestone", "data-testid": "poam-milestone", "data-milestone-id": "{milestone.id}", input { class: "input focus-ring", value: "{draft.title}", disabled: props.readonly, oninput: move |event| { let mut next = drafts.read().clone(); if let Some(value) = next.get_mut(&milestone.id) { value.title = event.value(); } drafts.set(next); } } input { class: "input focus-ring mono", r#type: "date", value: "{draft.target}", disabled: props.readonly, oninput: move |event| { let mut next = drafts.read().clone(); if let Some(value) = next.get_mut(&milestone.id) { value.target = event.value(); } drafts.set(next); } } button { class: "btn btn-ghost xs focus-ring", disabled: props.readonly, onclick: move |_| if let Some(value) = drafts.read().get(&milestone.id).cloned() { props.on_update.call((milestone.id, value, None)); }, "Save" } button { class: "btn btn-ghost xs focus-ring", disabled: props.readonly, onclick: move |_| if let Some(value) = drafts.read().get(&milestone.id).cloned() { props.on_update.call((milestone.id, value, Some(milestone.completed_at.is_none()))); }, if milestone.completed_at.is_some() { "Reopen" } else { "Complete" } } button { class: "btn-icon focus-ring", title: "Remove milestone", disabled: props.readonly, onclick: move |_| props.on_remove.call(milestone.id), Icon { name: IconName::Trash, size: 12 } } } } } }
        div { class: "poam-milestone-add", input { class: "input focus-ring", value: "{new_title}", placeholder: "Add milestone", disabled: props.readonly, oninput: move |event| new_title.set(event.value()) } input { class: "input focus-ring mono", r#type: "date", value: "{new_target}", disabled: props.readonly, oninput: move |event| new_target.set(event.value()) } button { class: "btn btn-ghost focus-ring", disabled: props.readonly || new_title.read().trim().is_empty() || new_target.read().is_empty(), onclick: move |_| props.on_add.call((new_title.read().trim().to_string(), new_target.read().clone())), Icon { name: IconName::Plus, size: 12 } "Add" } }
    } }
}

#[component]
fn ActivityList(activity: Vec<ActivityView>) -> Element {
    rsx! { div { class: "poam-activity", if activity.is_empty() { div { class: "poam-empty", "No durable activity has been recorded." } } for item in activity { { let payload = serde_json::to_string_pretty(&item.payload).unwrap_or_else(|_| "null".to_string()); rsx! { div { class: "poam-activity-row", time { class: "mono", "{item.created_at}" } strong { "{item.kind}" } pre { "{payload}" } } } } } } }
}

#[derive(Props, Clone, PartialEq)]
pub struct PoamCountStripProps {
    pub rollup: Rollup,
}

#[component]
pub fn PoamCountStrip(props: PoamCountStripProps) -> Element {
    let cells = [
        (
            "Open findings",
            props.rollup.open_findings,
            "poam-count-fail",
        ),
        ("On POA&M", props.rollup.on_poam_findings, "poam-count-info"),
        ("No POA&M", props.rollup.no_poam_findings, "poam-count-warn"),
        ("Overdue", props.rollup.overdue, "poam-count-fail"),
        (
            "Awaiting verification",
            props.rollup.awaiting_verification,
            "poam-count-awaiting",
        ),
        ("Closed", props.rollup.completed, "poam-count-ok"),
    ];
    rsx! { div { class: "stat-strip stat-strip-flush poam-count-strip", for (label, value, class) in cells { div { class: "stat", div { class: "stat-label", "{label}" } div { class: "stat-value {class}", "{value}" } } } } }
}

#[derive(Props, Clone, PartialEq)]
pub struct PoamTableProps {
    pub items: Vec<PoamSummary>,
    pub on_open: EventHandler<Uuid>,
    #[props(default = "No POA&M items in this view.".to_string())]
    pub empty_note: String,
}

#[component]
pub fn PoamTable(props: PoamTableProps) -> Element {
    if props.items.is_empty() {
        return rsx! { div { class: "poam-empty", "{props.empty_note}" } };
    }
    rsx! { div { class: "poam-table-wrap", table { class: "sys-table compact sys-table-dense poam-table", thead { tr { th { "POA&M" } th { "Title" } th { "Risk" } th { "Status" } th { "Owner" } th { "Due" } } } tbody { for item in props.items { tr { key: "{item.id}", tabindex: "0", "data-testid": "poam-row", "data-poam-id": "{item.id}", onclick: move |_| props.on_open.call(item.id), onkeydown: move |event| { let key = event.key(); if key == Key::Enter || matches!(key, Key::Character(ref value) if value == " ") { props.on_open.call(item.id); } }, td { class: "mono poam-human-id", "{item.human_id}" } td { strong { "{item.title}" } small { "{item.finding_count} linked findings" } } td { RiskChip { risk: item.risk } } td { StatusChip { poam: item.clone() } } td { "{item.owner}" } td { class: if item.overdue { "mono poam-overdue" } else { "mono" }, "{format_date(item.target_date)}" } } } } } } }
}

#[derive(Props, Clone, PartialEq)]
pub struct SystemPoamSectionProps {
    pub hostname: String,
    pub rollup: Rollup,
    pub items: Vec<PoamSummary>,
    pub filter: PoamFilter,
    pub on_filter: EventHandler<PoamFilter>,
    pub on_open: EventHandler<Uuid>,
}

#[component]
pub fn SystemPoamSection(props: SystemPoamSectionProps) -> Element {
    let filtered = props
        .items
        .iter()
        .filter(|item| props.filter.includes(item))
        .cloned()
        .collect();
    rsx! { section { class: "card poam-system-section", header { Icon { name: IconName::Gear, size: 14 } div { h2 { "POA&M" } p { "Remediation plans for {props.hostname}" } } FilterButtons { filter: props.filter, rollup: props.rollup.clone(), on_filter: props.on_filter } } PoamCountStrip { rollup: props.rollup } PoamTable { items: filtered, on_open: props.on_open, empty_note: "No POA&M items match this filter.".to_string() } } }
}

#[derive(Props, Clone, PartialEq)]
struct FilterButtonsProps {
    filter: PoamFilter,
    rollup: Rollup,
    on_filter: EventHandler<PoamFilter>,
}

#[component]
fn FilterButtons(props: FilterButtonsProps) -> Element {
    rsx! { div { class: "seg poam-filter", for option in [PoamFilter::Open, PoamFilter::Overdue, PoamFilter::Awaiting, PoamFilter::Closed, PoamFilter::All] { { let count = match option { PoamFilter::Open => props.rollup.active, PoamFilter::Overdue => props.rollup.overdue, PoamFilter::Awaiting => props.rollup.awaiting_verification, PoamFilter::Closed => props.rollup.completed, PoamFilter::All => props.rollup.total }; rsx! { button { class: if props.filter == option { "active" } else { "" }, onclick: move |_| props.on_filter.call(option), "{option.label()}" span { class: "mono", "{count}" } } } } } } }
}

#[derive(Props, Clone, PartialEq)]
pub struct BundlePoamRollupProps {
    pub bundle_name: String,
    pub rollup: Rollup,
    pub on_open_list: EventHandler<PoamFilter>,
}

#[component]
pub fn BundlePoamRollup(props: BundlePoamRollupProps) -> Element {
    rsx! { section { class: "poam-bundle-rollup", div { class: "poam-rollup-title", Icon { name: IconName::Gear, size: 14 } div { strong { "POA&M roll-up" } small { "Authoritative finding coverage for {props.bundle_name}" } } } div { class: "poam-rollup-counts", div { strong { class: "poam-count-fail", "{props.rollup.open_findings}" } span { "Open findings" } } div { strong { class: "poam-count-info", "{props.rollup.on_poam_findings}" } span { "On POA&M" } } div { strong { class: "poam-count-warn", "{props.rollup.no_poam_findings}" } span { "No POA&M" } } button { onclick: move |_| props.on_open_list.call(PoamFilter::Overdue), strong { class: "poam-count-fail", "{props.rollup.overdue}" } span { "Overdue" } } button { onclick: move |_| props.on_open_list.call(PoamFilter::Awaiting), strong { class: "poam-count-awaiting", "{props.rollup.awaiting_verification}" } span { "Awaiting" } } button { onclick: move |_| props.on_open_list.call(PoamFilter::Closed), strong { class: "poam-count-ok", "{props.rollup.completed}" } span { "Closed" } } button { class: "btn btn-ghost xs focus-ring", onclick: move |_| props.on_open_list.call(PoamFilter::All), "{props.rollup.total} POA&M items" Icon { name: IconName::ArrowRight, size: 11 } } } } }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(status: PoamStatus, overdue: bool) -> PoamSummary {
        PoamSummary {
            id: Uuid::nil(),
            human_id: "POAM-1".into(),
            title: "Test".into(),
            plan: String::new(),
            owner: "Security".into(),
            target_date: None,
            risk: PoamRisk::High,
            status,
            revision: 1,
            overdue,
            finding_count: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            closed_at: None,
            closure_attempt_id: None,
        }
    }

    #[test]
    fn labels_and_styles_cover_server_vocabulary() {
        assert_eq!(
            status_label(PoamStatus::AwaitingVerification),
            "Awaiting Verification"
        );
        assert_eq!(status_class(PoamStatus::Completed), "poam-chip-completed");
        assert_eq!(risk_label(PoamRisk::High), "CAT I - High");
        assert_eq!(risk_class(PoamRisk::Low), "poam-risk-low");
        assert_eq!(result_label(VerificationResult::Waiver), "Waiver");
        assert_eq!(
            result_class(VerificationResult::Waiver),
            "poam-result-waiver"
        );
        assert_ne!(
            result_class(VerificationResult::Waiver),
            result_class(VerificationResult::Pass)
        );
    }

    #[test]
    fn filters_use_summary_state_without_deriving_counts() {
        let open = summary(PoamStatus::Open, false);
        let overdue = summary(PoamStatus::Blocked, true);
        let awaiting = summary(PoamStatus::AwaitingVerification, false);
        let closed = summary(PoamStatus::Completed, false);
        assert!(PoamFilter::Open.includes(&open));
        assert!(PoamFilter::Overdue.includes(&overdue));
        assert!(PoamFilter::Awaiting.includes(&awaiting));
        assert!(PoamFilter::Closed.includes(&closed));
        assert!(!PoamFilter::Open.includes(&closed));
    }
}
