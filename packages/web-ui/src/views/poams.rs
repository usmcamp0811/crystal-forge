//! Authenticated, page-backed POA&M register.
//!
//! All counts and scope memberships here describe loaded pages only. Risk
//! acceptance decisions remain owned by their source services.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Days, NaiveDate, Utc};
use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;
use uuid::Uuid;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, closure::Closure};

use crate::api::client::fetch_environments;
use crate::api::models::EnvironmentSummary;
use crate::components::environments::looks_like_hex_color;
use crate::components::icon::{Icon, IconName};
use crate::components::poam::PoamDetailHost;
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::views::poam_api::{
    self, AcceptanceEntry, AcceptanceSource, CreatePoamRequest, FleetCvePoamRequest, PoamApiError,
    PoamAssigneeCatalog, PoamAssigneeRequest, PoamAssigneeView, PoamListQuery, PoamRegisterSummary,
    PoamRisk, PoamStatus, TransitionPoamRequest, UpdatePoamRequest,
};

const MAX_BULK_PLANS: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Everything,
    Plans,
    Acceptances,
}

impl Tab {
    fn key(self) -> &'static str {
        match self {
            Self::Everything => "all",
            Self::Plans => "poam",
            Self::Acceptances => "ra",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dimension {
    Environment,
    Bundle,
    Owner,
}

impl Dimension {
    fn key(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::Bundle => "bundle",
            Self::Owner => "owner",
        }
    }
    fn parse(value: &str) -> Self {
        match value {
            "bundle" => Self::Bundle,
            "owner" => Self::Owner,
            _ => Self::Environment,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Queue {
    Overdue,
    Soon,
    Awaiting,
    Blocked,
    Quiet,
    Unassigned,
    ExpiredAcceptance,
    ReviewSoon,
    NoReview,
    Accepted,
    ComingDue,
    Gaps,
}

impl Queue {
    fn key(self) -> &'static str {
        match self {
            Self::Overdue => "late",
            Self::Soon => "soon",
            Self::Awaiting => "awaiting",
            Self::Blocked => "blocked",
            Self::Quiet => "stale",
            Self::Unassigned => "unassigned",
            Self::ExpiredAcceptance => "expired",
            Self::ReviewSoon => "review_soon",
            Self::NoReview => "no_review",
            Self::Accepted => "accepted",
            Self::ComingDue => "coming_due",
            Self::Gaps => "gaps",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "late" => Some(Self::Overdue),
            "soon" => Some(Self::Soon),
            "awaiting" => Some(Self::Awaiting),
            "blocked" => Some(Self::Blocked),
            "stale" => Some(Self::Quiet),
            "unassigned" => Some(Self::Unassigned),
            "expired" => Some(Self::ExpiredAcceptance),
            "review_soon" => Some(Self::ReviewSoon),
            "no_review" => Some(Self::NoReview),
            "accepted" => Some(Self::Accepted),
            "coming_due" => Some(Self::ComingDue),
            "gaps" => Some(Self::Gaps),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Overdue => "Overdue plans",
            Self::Soon => "Due in 14 days",
            Self::Awaiting => "Awaiting verification",
            Self::Blocked => "Blocked",
            Self::Quiet => "No activity in 30 days",
            Self::Unassigned => "Unassigned",
            Self::ExpiredAcceptance => "Expired acceptances",
            Self::ReviewSoon => "Review in 30 days",
            Self::NoReview => "No review date",
            Self::Accepted => "Accepted decisions",
            Self::ComingDue => "Coming due",
            Self::Gaps => "Missing owner/review",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Overdue => "Past target completion",
            Self::Soon => "Coming up next",
            Self::Awaiting => "Re-evaluate to close",
            Self::Blocked => "Dependency to clear",
            Self::Quiet => "Open but quiet",
            Self::Unassigned => "Needs an owner",
            Self::ExpiredAcceptance => "Renew or plan a fix",
            Self::ReviewSoon => "Re-review before expiry",
            Self::NoReview => "Assessors flag these",
            Self::Accepted => "Source-approved risk",
            Self::ComingDue => "Plans ≤14d · reviews ≤30d",
            Self::Gaps => "Unassigned or undated",
        }
    }

    fn includes(self, row: &PoamRegisterSummary, today: NaiveDate) -> bool {
        let p = &row.summary;
        if !p.status.is_active() {
            return false;
        }
        match self {
            Self::Overdue => p.target_date.is_some_and(|date| date < today),
            Self::Soon => p
                .target_date
                .is_some_and(|date| date >= today && (date - today).num_days() <= 14),
            Self::Awaiting => p.status == PoamStatus::AwaitingVerification,
            Self::Blocked => p.status == PoamStatus::Blocked,
            Self::Quiet => row
                .last_activity_at
                .is_some_and(|at| (Utc::now() - at).num_days() >= 30),
            Self::Unassigned => matches!(p.assignee, None | Some(PoamAssigneeView::Unassigned)),
            Self::ComingDue => p
                .target_date
                .is_some_and(|date| date >= today && (date - today).num_days() <= 14),
            Self::Gaps => matches!(p.assignee, None | Some(PoamAssigneeView::Unassigned)),
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Scope {
    Environment(Uuid),
    System(Uuid),
    Bundle(Uuid),
    Owner(Uuid),
}

/// The URL holds only validated, typed register focus. Unknown values do not
/// broaden server authorization; they are ignored rather than sent as filters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RegisterLocation {
    poam: Option<Uuid>,
    kind: Tab,
    queue: Option<Queue>,
    scope: Option<Scope>,
    dimension: Dimension,
}

impl RegisterLocation {
    fn parse(query: &str) -> Self {
        let mut state = Self {
            poam: None,
            kind: Tab::Everything,
            queue: None,
            scope: None,
            dimension: Dimension::Environment,
        };
        let Ok(pairs) =
            serde_urlencoded::from_str::<Vec<(String, String)>>(query.trim_start_matches('?'))
        else {
            return state;
        };
        let explicit_dimension = pairs.iter().any(|(name, _)| name == "dim");
        for (name, value) in pairs {
            match name.as_str() {
                "poam" => state.poam = Uuid::parse_str(&value).ok(),
                "kind" => {
                    state.kind = match value.as_str() {
                        "poam" => Tab::Plans,
                        "ra" => Tab::Acceptances,
                        _ => Tab::Everything,
                    }
                }
                "queue" => state.queue = Queue::parse(&value),
                "dim" => state.dimension = Dimension::parse(&value),
                "environment" => {
                    if let Ok(id) = Uuid::parse_str(&value) {
                        state.scope = Some(Scope::Environment(id));
                    }
                }
                "system" => {
                    if let Ok(id) = Uuid::parse_str(&value) {
                        state.scope = Some(Scope::System(id));
                    }
                }
                "bundle" => {
                    if let Ok(id) = Uuid::parse_str(&value) {
                        state.scope = Some(Scope::Bundle(id));
                    }
                }
                "owner" => {
                    if let Ok(id) = Uuid::parse_str(&value) {
                        state.scope = Some(Scope::Owner(id));
                    }
                }
                _ => {}
            }
        }
        if !explicit_dimension {
            state.dimension = match state.scope {
                Some(Scope::Bundle(_)) => Dimension::Bundle,
                Some(Scope::Owner(_)) => Dimension::Owner,
                _ => Dimension::Environment,
            };
        }
        state
    }

    fn query(self) -> String {
        let mut pairs = Vec::new();
        if let Some(id) = self.poam {
            pairs.push(("poam", id.to_string()));
        }
        if self.kind != Tab::Everything {
            pairs.push(("kind", self.kind.key().into()));
        }
        if let Some(queue) = self.queue {
            pairs.push(("queue", queue.key().into()));
        }
        if self.dimension != Dimension::Environment {
            pairs.push(("dim", self.dimension.key().into()));
        }
        if let Some(scope) = self.scope {
            let (name, id) = match scope {
                Scope::Environment(id) => ("environment", id),
                Scope::System(id) => ("system", id),
                Scope::Bundle(id) => ("bundle", id),
                Scope::Owner(id) => ("owner", id),
            };
            pairs.push((name, id.to_string()));
        }
        serde_urlencoded::to_string(pairs).unwrap_or_default()
    }

    fn list_query(self, offset: i64) -> PoamListQuery {
        let mut query = PoamListQuery {
            limit: Some(100),
            offset: Some(offset),
            ..Default::default()
        };
        match self.scope {
            Some(Scope::System(id)) => query.system_id = Some(id),
            Some(Scope::Bundle(id)) => query.bundle_id = Some(id),
            _ => {}
        }
        query
    }

    fn partial_scope(self) -> bool {
        matches!(
            self.scope,
            Some(Scope::Environment(_) | Scope::Owner(_) | Scope::Bundle(_))
        )
    }

    fn selection_key(self) -> String {
        Self { poam: None, ..self }.query()
    }
}

fn in_scope(row: &PoamRegisterSummary, scope: Option<Scope>) -> bool {
    match scope {
        None => true,
        Some(Scope::Environment(id)) => row.environment_ids.contains(&id),
        Some(Scope::System(id)) => row.system_ids.contains(&id),
        Some(Scope::Bundle(id)) => row.bundle_ids.contains(&id),
        Some(Scope::Owner(id)) => {
            matches!(&row.summary.assignee, Some(PoamAssigneeView::User { user_id, .. }) if *user_id == id)
        }
    }
}

// Selection follows the displayed, deduplicated loaded order; stale anchors
// never select a range from a previous filter or page window.
fn select_row(
    selected: &mut BTreeSet<Uuid>,
    anchor: &mut Option<Uuid>,
    id: Uuid,
    order: &[Uuid],
    shift: bool,
    toggle: bool,
) -> bool {
    if shift || toggle {
        if shift {
            if let (Some(start), Some(end)) = (
                anchor.and_then(|a| order.iter().position(|x| *x == a)),
                order.iter().position(|x| *x == id),
            ) {
                for key in &order[start.min(end)..=start.max(end)] {
                    selected.insert(*key);
                }
            } else {
                selected.insert(id);
                *anchor = Some(id);
            }
        } else {
            if !selected.insert(id) {
                selected.remove(&id);
            }
            *anchor = Some(id);
        }
        true
    } else {
        *anchor = Some(id);
        false
    }
}

fn owner(row: &PoamRegisterSummary) -> String {
    match &row.summary.assignee {
        Some(
            PoamAssigneeView::User { display, .. }
            | PoamAssigneeView::OidcGroup { display, .. }
            | PoamAssigneeView::Legacy { display },
        ) => display.clone(),
        _ => "Unassigned".into(),
    }
}

fn approver_label(id: Option<Uuid>, catalog: Option<&PoamAssigneeCatalog>) -> String {
    id.and_then(|id| catalog?.people.iter().find(|person| person.user_id == id))
        .map(|person| person.label.clone())
        .unwrap_or_else(|| "Approver unavailable".into())
}

fn group_title(key: &str) -> &str {
    if key.starts_with("Host ") {
        "System name unavailable"
    } else {
        key
    }
}

fn system_group_name(
    key: &str,
    plans: &[&PoamRegisterSummary],
    decisions: &[&AcceptanceEntry],
) -> String {
    let Some(id) = key
        .strip_prefix("Host ")
        .and_then(|id| Uuid::parse_str(id).ok())
    else {
        return group_title(key).to_string();
    };
    plans
        .iter()
        .flat_map(|row| &row.systems)
        .find(|system| system.system_id == id)
        .map(|system| system.hostname.clone())
        .or_else(|| {
            decisions
                .iter()
                .find(|item| item.system_id == Some(id))
                .and_then(|item| item.system_hostname.clone())
        })
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "System name unavailable".into())
}

// A plan can appear in several environment groups. A selected host uses its
// own visible system edge even when the displayed group excludes that member.
// Without a selected host, only members of the displayed group contribute.
fn plan_subtitle(
    row: &PoamRegisterSummary,
    environment: Option<Uuid>,
    environment_grouped: bool,
    selected_system: Option<Uuid>,
    environments: &[EnvironmentSummary],
) -> String {
    let members: Vec<_> = row
        .systems
        .iter()
        .filter(|system| {
            if let Some(id) = selected_system {
                system.system_id == id
            } else {
                !environment_grouped
                    || environment == system.environment_id && environment.is_some()
            }
        })
        .collect();
    let mut parts = Vec::new();
    if members.len() == 1 && !members[0].hostname.trim().is_empty() {
        parts.push(members[0].hostname.trim().to_string());
    } else if members.len() == 1 {
        parts.push("1 host".into());
    } else if !members.is_empty() {
        parts.push(format!("{} hosts", members.len()));
    } else if selected_system.is_some_and(|id| row.system_ids.contains(&id)) {
        parts.push("1 host".into());
    } else if !environment_grouped && !row.system_ids.is_empty() && selected_system.is_none() {
        parts.push(format!(
            "{} {}",
            row.system_ids.len(),
            if row.system_ids.len() == 1 {
                "host"
            } else {
                "hosts"
            }
        ));
    }
    let scoped_environment = if selected_system.is_some() {
        members.first().and_then(|system| system.environment_id)
    } else if environment_grouped {
        environment
    } else {
        (row.environment_ids.len() == 1).then(|| row.environment_ids[0])
    };
    if let Some(id) = scoped_environment {
        parts.push(environment_name(id, environments));
    }
    // The first evidence ID is page-wide. On a multi-environment plan it
    // cannot be attributed to one group, so omit it rather than mislabel it.
    if row.environment_ids.len() <= 1 {
        if let Some(identity) = row
            .first_requirement
            .as_deref()
            .or(row.first_cve.as_deref())
        {
            parts.push(identity.to_string());
        }
    }
    if parts.is_empty() {
        "Scope unavailable".into()
    } else {
        parts.join(" · ")
    }
}

// A host decision does not establish environment membership. Compose its title
// from the source's direct scope only, even when that host is in a loaded plan.
fn acceptance_row_title(item: &AcceptanceEntry, environments: &[EnvironmentSummary]) -> String {
    let host = item
        .system_hostname
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let environment = item
        .environment_id
        .map(|id| environment_name(id, environments));
    let scope = match item.source {
        AcceptanceSource::CveHost => host.map(|name| format!(" on {name}")),
        AcceptanceSource::CveEnvironment => environment.map(|name| format!(" in {name}")),
        AcceptanceSource::PolicyWaiver => host
            .map(|name| format!(" on {name}"))
            .or_else(|| environment.map(|name| format!(" in {name}"))),
    }
    .unwrap_or_default();
    match item.canonical_cve_id.as_deref() {
        Some(cve) => format!(
            "{cve} — {}{scope}",
            item.canonical_package_name
                .as_deref()
                .unwrap_or("package unavailable")
        ),
        None if item.finding_id.is_some() => format!(
            "{}{scope}",
            item.policy_title.as_deref().unwrap_or("Policy finding")
        ),
        None => format!("Policy waiver{scope}"),
    }
}

fn acceptance_source_label(source: AcceptanceSource) -> &'static str {
    match source {
        AcceptanceSource::PolicyWaiver => "Policy waiver",
        AcceptanceSource::CveHost => "Host CVE",
        AcceptanceSource::CveEnvironment => "Environment CVE",
    }
}

fn acceptance_review_relative(item: &AcceptanceEntry, today: NaiveDate) -> String {
    match review_deadline(item).map(|date| (date - today).num_days()) {
        Some(days) if days < 0 => format!("expired {}d ago", -days),
        Some(0) => "due today".into(),
        Some(days) => format!("in {days}d"),
        None => "No review date".into(),
    }
}

fn environment_color(id: Uuid, environments: &[EnvironmentSummary]) -> Option<&str> {
    environments
        .iter()
        .find(|env| env.id == id)
        .map(|env| env.color_hex.as_str())
        .filter(|color| looks_like_hex_color(color))
}

// Group Focus is valid only if all displayed decisions carry that same direct
// source scope. A name match alone cannot authorize navigation to an environment.
fn acceptance_group_focus(group_by: &str, items: &[AcceptanceEntry]) -> Option<Scope> {
    let mut scopes = items.iter().map(|item| match group_by {
        "environment" => item.environment_id.map(Scope::Environment),
        "system" => item.system_id.map(Scope::System),
        "owner" => item.accepted_by.map(Scope::Owner),
        _ => None,
    });
    let first = scopes.next().flatten()?;
    scopes.all(|scope| scope == Some(first)).then_some(first)
}

fn group_distribution(
    plans: &[&PoamRegisterSummary],
    decisions: &[&AcceptanceEntry],
) -> Vec<(&'static str, &'static str, usize)> {
    let mut segments = Vec::new();
    for (status, label, color) in [
        (PoamStatus::Open, "Open", "#60a5fa"),
        (PoamStatus::InProgress, "In progress", "#fbbf24"),
        (PoamStatus::Blocked, "Blocked", "#f97316"),
        (
            PoamStatus::AwaitingVerification,
            "Awaiting verification",
            "#a78bfa",
        ),
        (PoamStatus::Completed, "Completed", "#34d399"),
    ] {
        let count = plans
            .iter()
            .filter(|row| row.summary.status == status)
            .count();
        if count > 0 {
            segments.push((label, color, count));
        }
    }
    for (label, color) in [
        ("Risk accepted", "#a78bfa"),
        ("Authorization expired", "#f87171"),
        ("Retired", "#9ca3af"),
        ("Converted to POA&M", "#60a5fa"),
        ("Decision pending", "#fbbf24"),
    ] {
        let count = decisions
            .iter()
            .filter(|item| acceptance_status(item) == label)
            .count();
        if count > 0 {
            segments.push((label, color, count));
        }
    }
    segments
}

// An acceptance names either its environment directly or its host directly.
// A host-only source never establishes a host-to-environment edge.
fn acceptance_group_keys(
    item: &AcceptanceEntry,
    group: &str,
    environments: &[EnvironmentSummary],
    catalog: Option<&PoamAssigneeCatalog>,
    today: NaiveDate,
) -> Vec<String> {
    match group {
        "environment" => vec![
            item.environment_id
                .map(|id| environment_name(id, environments))
                .unwrap_or_else(|| {
                    if item.system_id.is_some() {
                        "Host-only decisions"
                    } else {
                        "No host scope"
                    }
                    .into()
                }),
        ],
        "system" => vec![
            item.system_id
                .map(|id| format!("Host {id}"))
                .unwrap_or_else(|| {
                    if item.environment_id.is_some() {
                        "Environment decisions"
                    } else {
                        "No host scope"
                    }
                    .into()
                }),
        ],
        "bundle" => vec!["No bundle attribution".into()],
        "owner" => vec![approver_label(item.accepted_by, catalog)],
        "type" => vec!["Risk acceptances".into()],
        "due" => vec![if item.status != "accepted" {
            "Closed".into()
        } else if item
            .review_due_at
            .or(item.review_date)
            .is_some_and(|due| due < today)
        {
            "Late".into()
        } else {
            "Upcoming / no date".into()
        }],
        _ => vec!["Records".into()],
    }
}

fn acceptance_matches(
    item: &AcceptanceEntry,
    location: RegisterLocation,
    today: NaiveDate,
    status: &str,
    risk: &str,
    mine: bool,
    me: Option<Uuid>,
    query: &str,
    environments: &[EnvironmentSummary],
) -> bool {
    acceptance_in_scope(item, location.scope)
        && acceptance_queue(item, location.queue, today)
        && (status == "all" || (status == "active") == (item.retired_at.is_none()
            && (item.status == "accepted" || (location.queue == Some(Queue::ExpiredAcceptance) && item.status == "expired"))))
        // Acceptance sources have no recorded plan-risk category.
        && risk == "all"
        && (!mine || me.is_some_and(|id| item.accepted_by == Some(id)))
        && (query.trim().is_empty() || [item.human_id.clone(), acceptance_row_title(item, environments), item.justification.clone(), item.system_hostname.clone().unwrap_or_default(), item.environment_name.clone().unwrap_or_default(), item.requirement_external_id.clone().unwrap_or_default()]
            .iter().any(|value| value.to_lowercase().contains(&query.trim().to_lowercase())))
}

fn acceptance_compare(a: &AcceptanceEntry, b: &AcceptanceEntry, sort: &str) -> std::cmp::Ordering {
    let due = || {
        a.review_due_at
            .or(a.review_date)
            .cmp(&b.review_due_at.or(b.review_date))
    };
    match sort {
        "activity" => b.recorded_at.cmp(&a.recorded_at),
        "id" => a.human_id.cmp(&b.human_id),
        // The source has no severity. Within a decision group, urgency and
        // risk sorting retain the recorded review-date order.
        _ => due(),
    }
    .then_with(|| a.source_id.cmp(&b.source_id))
}

fn environment_name(id: Uuid, environments: &[EnvironmentSummary]) -> String {
    environments
        .iter()
        .find(|environment| environment.id == id)
        .map(|environment| environment.name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| "Environment unavailable".into())
}

fn scope_pills(
    rows: &[PoamRegisterSummary],
    dimension: &str,
    current: Option<Scope>,
    environments: &[EnvironmentSummary],
    acceptances: &[AcceptanceEntry],
    catalog: Option<&PoamAssigneeCatalog>,
) -> Vec<(Scope, String, usize)> {
    let mut keys: BTreeMap<Scope, (String, BTreeSet<String>)> = BTreeMap::new();
    for row in rows {
        let candidates: Vec<(Scope, String)> = match (dimension, current) {
            ("environment", Some(Scope::Environment(env))) => {
                // Only a visible system edge proves membership in this
                // environment; plan-wide ID lists do not form host pairs.
                row.systems
                    .iter()
                    .filter(|system| {
                        system.environment_id == Some(env)
                            && row.system_ids.contains(&system.system_id)
                    })
                    .map(|system| {
                        (
                            Scope::System(system.system_id),
                            if system.hostname.trim().is_empty() {
                                "System name unavailable".into()
                            } else {
                                system.hostname.trim().to_string()
                            },
                        )
                    })
                    .collect()
            }
            ("environment", None) => row
                .environment_ids
                .iter()
                .map(|id| (Scope::Environment(*id), environment_name(*id, environments)))
                .collect(),
            ("bundle", None) => row
                .bundle_ids
                .iter()
                .map(|id| (Scope::Bundle(*id), format!("Bundle {id}")))
                .collect(),
            ("owner", None) => match &row.summary.assignee {
                Some(PoamAssigneeView::User {
                    user_id, display, ..
                }) => vec![(Scope::Owner(*user_id), display.clone())],
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        for (key, label) in candidates {
            let entry = keys.entry(key).or_insert_with(|| (label, BTreeSet::new()));
            entry.1.insert(format!("poam:{}", row.summary.id));
        }
    }
    // A host-only acceptance does not establish an environment membership.
    // Only add direct environment/approver identities recorded by its source.
    for item in acceptances {
        let candidate = match (dimension, current) {
            ("environment", None) => item
                .environment_id
                .map(|id| (Scope::Environment(id), environment_name(id, environments))),
            ("owner", None) => item
                .accepted_by
                .map(|id| (Scope::Owner(id), approver_label(Some(id), catalog))),
            _ => None,
        };
        if let Some((scope, label)) = candidate {
            keys.entry(scope)
                .or_insert_with(|| (label, BTreeSet::new()))
                .1
                .insert(format!("{}:{}", item.source.key(), item.source_id));
        }
    }
    let mut pills: Vec<_> = keys
        .into_iter()
        .map(|(key, (label, ids))| (key, label, ids.len()))
        .collect();
    pills.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
    pills
}

fn displayed_order(
    groups: &BTreeMap<String, Vec<&PoamRegisterSummary>>,
    collapsed: &BTreeSet<String>,
    limit: usize,
) -> Vec<Uuid> {
    let mut seen = BTreeSet::new();
    groups
        .iter()
        .filter(|(name, _)| !collapsed.contains(*name))
        .flat_map(|(_, entries)| entries.iter().take(limit).map(|row| row.summary.id))
        .filter(|id| seen.insert(*id))
        .collect()
}

fn legal_transition(from: PoamStatus, to: PoamStatus) -> bool {
    match from {
        PoamStatus::Open | PoamStatus::InProgress | PoamStatus::Blocked => {
            from != to
                && matches!(
                    to,
                    PoamStatus::Open
                        | PoamStatus::InProgress
                        | PoamStatus::Blocked
                        | PoamStatus::AwaitingVerification
                )
        }
        PoamStatus::AwaitingVerification => {
            matches!(to, PoamStatus::InProgress | PoamStatus::Blocked)
        }
        PoamStatus::Completed => false,
    }
}

fn status_key(status: PoamStatus) -> &'static str {
    match status {
        PoamStatus::Open => "open",
        PoamStatus::InProgress => "in_progress",
        PoamStatus::Blocked => "blocked",
        PoamStatus::AwaitingVerification => "awaiting_verification",
        PoamStatus::Completed => "completed",
    }
}

fn extended_date(date: Option<NaiveDate>) -> Option<NaiveDate> {
    date?.checked_add_days(Days::new(30))
}

#[derive(Clone)]
enum BulkAction {
    Assign(PoamAssigneeRequest),
    Status(PoamStatus),
    Extend,
}

fn eligible(row: &PoamRegisterSummary, action: &BulkAction) -> bool {
    row.summary.status.is_active()
        && match action {
            BulkAction::Assign(_) => true,
            BulkAction::Status(to) => legal_transition(row.summary.status, *to),
            BulkAction::Extend => extended_date(row.summary.target_date).is_some(),
        }
}

fn batch_targets(
    rows: &[PoamRegisterSummary],
    selected: &BTreeSet<Uuid>,
    action: &BulkAction,
) -> Option<Vec<(Uuid, i64, Option<NaiveDate>)>> {
    if selected.is_empty() || selected.len() > MAX_BULK_PLANS {
        return None;
    }
    let targets: Vec<_> = rows
        .iter()
        .filter(|row| selected.contains(&row.summary.id))
        .map(|row| {
            (
                row.summary.id,
                row.summary.revision,
                row.summary.target_date,
            )
        })
        .collect();
    (targets.len() == selected.len()
        && targets.iter().all(|(id, _, _)| {
            rows.iter()
                .find(|row| row.summary.id == *id)
                .is_some_and(|row| eligible(row, action))
        }))
    .then_some(targets)
}

#[derive(Clone, Default)]
struct BulkOutcome {
    succeeded: BTreeSet<Uuid>,
    failed: Vec<(Uuid, String)>,
}

fn record_result(outcome: &mut BulkOutcome, id: Uuid, result: Result<(), String>) {
    match result {
        Ok(()) => {
            outcome.succeeded.insert(id);
        }
        Err(reason) => outcome.failed.push((id, reason)),
    }
}

fn remaining_selection(selected: &BTreeSet<Uuid>, outcome: &BulkOutcome) -> BTreeSet<Uuid> {
    selected.difference(&outcome.succeeded).copied().collect()
}

fn assignee_from_catalog(
    value: &str,
    catalog: &PoamAssigneeCatalog,
) -> Option<PoamAssigneeRequest> {
    if value == "unassigned" {
        return Some(PoamAssigneeRequest::Unassigned);
    }
    if let Some(id) = value
        .strip_prefix("user:")
        .and_then(|id| Uuid::parse_str(id).ok())
    {
        return catalog
            .people
            .iter()
            .any(|person| person.user_id == id)
            .then_some(PoamAssigneeRequest::User { user_id: id });
    }
    let group = value.strip_prefix("group:")?;
    catalog
        .groups
        .iter()
        .any(|item| item.group_name == group)
        .then(|| PoamAssigneeRequest::OidcGroup {
            group_name: group.to_string(),
        })
}

async fn apply_bulk_action(
    targets: &[(Uuid, i64, Option<NaiveDate>)],
    action: &BulkAction,
) -> BulkOutcome {
    let mut outcome = BulkOutcome::default();
    // Each request carries the revision shown when the batch started. A failed
    // request is not retried: a transport error may follow a committed write.
    for &(id, revision, date) in targets {
        let result = match action {
            BulkAction::Assign(assignee) => {
                poam_api::update_poam(
                    id,
                    &UpdatePoamRequest {
                        revision,
                        assignee: Some(assignee.clone()),
                        ..Default::default()
                    },
                )
                .await
            }
            BulkAction::Status(status) => {
                poam_api::transition_poam(
                    id,
                    &TransitionPoamRequest {
                        revision,
                        status: *status,
                        note: None,
                    },
                )
                .await
            }
            BulkAction::Extend => {
                poam_api::update_poam(
                    id,
                    &UpdatePoamRequest {
                        revision,
                        target_date: Some(extended_date(date)),
                        ..Default::default()
                    },
                )
                .await
            }
        };
        record_result(
            &mut outcome,
            id,
            result.map(|_| ()).map_err(|error| error.to_string()),
        );
    }
    outcome
}

async fn refresh_loaded_pages(
    location: RegisterLocation,
    count: usize,
) -> Result<(Vec<PoamRegisterSummary>, Option<i64>), PoamApiError> {
    let mut items = Vec::new();
    let mut offset = Some(0);
    // Re-read at least the number of pages previously loaded. Fresh server
    // ordering can move updated rows between pages; never keep old revisions.
    let page_count = count.div_ceil(MAX_BULK_PLANS).max(1);
    for _ in 0..page_count {
        let Some(at) = offset else {
            break;
        };
        let page = poam_api::list_poam_register(&location.list_query(at)).await?;
        items.extend(page.items);
        offset = page.next_offset;
    }
    let mut seen = BTreeSet::new();
    items.retain(|row| seen.insert(row.summary.id));
    Ok((items, offset))
}

fn acceptance_in_scope(item: &AcceptanceEntry, scope: Option<Scope>) -> bool {
    match scope {
        None => true,
        // Host decisions without an environment edge cannot be attributed to
        // an environment from the flat decision projection.
        Some(Scope::Environment(id)) => item.environment_id == Some(id),
        Some(Scope::System(id)) => item.system_id == Some(id),
        Some(Scope::Owner(id)) => item.accepted_by == Some(id),
        // A decision is not evidence that any particular bundle applies.
        Some(Scope::Bundle(_)) => false,
    }
}

fn acceptance_subject(item: &AcceptanceEntry) -> String {
    if let Some(cve) = &item.canonical_cve_id {
        format!(
            "{cve} · {}",
            item.canonical_package_name
                .as_deref()
                .unwrap_or("Unknown package")
        )
    } else if let Some(finding) = item.finding_id {
        match item.policy_version_id {
            Some(version) => format!("Finding {finding} · policy version {version}"),
            None => format!("Finding {finding} · policy version unavailable"),
        }
    } else {
        "Subject unavailable".into()
    }
}

fn acceptance_status(item: &AcceptanceEntry) -> &'static str {
    if item.replacement_poam_id.is_some() {
        "Converted to POA&M"
    } else if item.status == "expired" || item.expires_at.is_some_and(|at| at <= Utc::now()) {
        "Authorization expired"
    } else if item.retired_at.is_some() || item.status == "revoked" {
        "Retired"
    } else if item.status == "accepted" {
        "Risk accepted"
    } else {
        "Decision pending"
    }
}

fn acceptance_status_class(status: &str) -> &'static str {
    match status {
        "Risk accepted" => "poams-accepted",
        "Authorization expired" | "Decision pending" => "poams-acceptance-warning",
        "Retired" => "poams-acceptance-retired",
        "Converted to POA&M" => "poams-acceptance-converted",
        _ => "poams-acceptance-retired",
    }
}

fn display_acceptance_date(date: NaiveDate) -> String {
    date.format("%b %-d, %Y").to_string()
}

fn display_acceptance_timestamp(at: chrono::DateTime<chrono::Utc>) -> String {
    display_acceptance_date(at.date_naive())
}

fn review_deadline(item: &AcceptanceEntry) -> Option<NaiveDate> {
    item.review_due_at.or(item.review_date)
}

fn review_is_expired(item: &AcceptanceEntry, today: NaiveDate) -> bool {
    review_deadline(item).is_some_and(|deadline| deadline < today)
}

fn acceptance_drawer_subject(item: &AcceptanceEntry) -> String {
    let mut parts = Vec::new();
    if let Some(title) = item.policy_title.as_deref() {
        parts.push(title.to_string());
    }
    if let Some(requirement) = item.requirement_external_id.as_deref() {
        parts.push(requirement.to_string());
    }
    if let Some(cve) = item.canonical_cve_id.as_deref() {
        parts.push(cve.to_string());
    }
    if let Some(package) = item.canonical_package_name.as_deref() {
        parts.push(package.to_string());
    }
    if let Some(host) = item
        .system_hostname
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        parts.push(host.to_string());
    }
    if let Some(environment) = item.environment_name.as_deref() {
        parts.push(environment.to_string());
    }
    if parts.is_empty() {
        match item.source {
            AcceptanceSource::PolicyWaiver => parts.push("Policy finding".into()),
            AcceptanceSource::CveHost => parts.push("Host scope unavailable".into()),
            AcceptanceSource::CveEnvironment => parts.push("Environment scope unavailable".into()),
        }
    }
    parts.join(" · ")
}

fn acceptance_queue(item: &AcceptanceEntry, queue: Option<Queue>, today: NaiveDate) -> bool {
    let due = item.review_due_at.or(item.review_date);
    let active = item.status == "accepted"
        && item.retired_at.is_none()
        && item.replacement_poam_id.is_none();
    match queue {
        None => true,
        Some(Queue::ExpiredAcceptance) => {
            (active
                && (due.is_some_and(|date| date < today)
                    || item.expires_at.is_some_and(|at| at.date_naive() < today)))
                || (item.source == AcceptanceSource::PolicyWaiver
                    && item.status == "expired"
                    && item.retired_at.is_none()
                    && item.replacement_poam_id.is_none())
        }
        Some(Queue::ReviewSoon) => {
            active && due.is_some_and(|date| date >= today && (date - today).num_days() <= 30)
        }
        Some(Queue::NoReview) => active && due.is_none(),
        Some(Queue::Accepted) => active,
        Some(Queue::ComingDue) => {
            active && due.is_some_and(|date| date >= today && (date - today).num_days() <= 30)
        }
        Some(Queue::Gaps) => active && due.is_none(),
        _ => false,
    }
}

type AcceptanceId = (AcceptanceSource, Uuid);

fn acceptance_id(item: &AcceptanceEntry) -> AcceptanceId {
    (item.source, item.source_id)
}

fn renewable(item: &AcceptanceEntry, operator: bool, admin: bool) -> bool {
    item.status == "accepted"
        && item.retired_at.is_none()
        && item.replacement_poam_id.is_none()
        && match item.source {
            AcceptanceSource::PolicyWaiver => {
                admin
                    && item.waiver_updated_at.is_some()
                    && item.expires_at.is_none_or(|expiry| expiry > Utc::now())
            }
            AcceptanceSource::CveHost | AcceptanceSource::CveEnvironment => operator,
        }
}

async fn refresh_acceptances(
    count: usize,
    environment_id: Option<Uuid>,
    search: &str,
) -> Result<(Vec<AcceptanceEntry>, i64, bool), PoamApiError> {
    let mut items = Vec::new();
    let mut total = 0;
    let mut more = false;
    for offset in (0..count.div_ceil(100).max(1)).map(|page| (page * 100) as i64) {
        let page = poam_api::list_acceptances(offset, environment_id, search).await?;
        total = page.total;
        more = page.has_more;
        items.extend(page.items);
        if !more {
            break;
        }
    }
    let mut seen = BTreeSet::new();
    items.retain(|entry| seen.insert(acceptance_id(entry)));
    Ok((items, total, more))
}

#[component]
fn AcceptanceTray(
    entry: AcceptanceEntry,
    environments: Vec<EnvironmentSummary>,
    operator: bool,
    admin: bool,
    catalog: Option<PoamAssigneeCatalog>,
    on_close: EventHandler<()>,
    on_changed: EventHandler<()>,
) -> Element {
    let mut converting = use_signal(|| false);
    let mut confirming = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut title = use_signal(String::new);
    let mut plan = use_signal(String::new);
    let mut owner = use_signal(String::new);
    let mut assignee = use_signal(String::new);
    let mut risk = use_signal(String::new);
    let mut due = use_signal(String::new);
    let mut reuse = use_signal(String::new);
    let nav = use_navigator();
    #[cfg(target_arch = "wasm32")]
    {
        let listener = use_hook(move || {
            let callback = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
                move |event: web_sys::KeyboardEvent| {
                    if event.key() == "Escape" && !*busy.peek() {
                        on_close.call(());
                    }
                },
            );
            if let Some(window) = web_sys::window() {
                let _ = window
                    .add_event_listener_with_callback("keydown", callback.as_ref().unchecked_ref());
            }
            Rc::new(callback)
        });
        let remove = listener.clone();
        use_drop(move || {
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback(
                    "keydown",
                    remove.as_ref().as_ref().unchecked_ref(),
                );
            }
        });
    }
    let active = (entry.status == "accepted"
        || (entry.source == AcceptanceSource::PolicyWaiver && entry.status == "expired"))
        && entry.retired_at.is_none()
        && entry.replacement_poam_id.is_none();
    let can_renew = renewable(&entry, operator, admin);
    let can_convert = active
        && match entry.source {
            AcceptanceSource::PolicyWaiver => admin && entry.waiver_updated_at.is_some(),
            _ => operator,
        };
    let date = NaiveDate::parse_from_str(&due(), "%Y-%m-%d")
        .ok()
        .filter(|d| *d >= Utc::now().date_naive());
    let parsed_risk = match risk().as_str() {
        "high" => Some(PoamRisk::High),
        "medium" => Some(PoamRisk::Medium),
        "low" => Some(PoamRisk::Low),
        _ => None,
    };
    let typed = catalog
        .as_ref()
        .and_then(|c| assignee_from_catalog(&assignee(), c));
    let reuse_id = if reuse().trim().is_empty() {
        Some(None)
    } else {
        Uuid::parse_str(reuse().trim()).ok().map(Some)
    };
    let valid = !title().trim().is_empty()
        && !plan().trim().is_empty()
        && date.is_some()
        && parsed_risk.is_some()
        && reuse_id.is_some()
        && match entry.source {
            AcceptanceSource::PolicyWaiver => !owner().trim().is_empty() || typed.is_some(),
            _ => {
                typed
                    .as_ref()
                    .is_some_and(|choice| !matches!(choice, PoamAssigneeRequest::Unassigned))
                    && reuse_id == Some(None)
            }
        };
    let validation = if !valid {
        "Provide title, remediation plan, future due date, risk and a valid owner or typed assignee. CVE plans require a selected user or group; only policy plans can reuse a compatible plan UUID."
    } else {
        "Review the metadata before confirming. The source service checks current evidence and compatibility."
    };
    let label = match entry.source {
        AcceptanceSource::PolicyWaiver => "Policy waiver",
        AcceptanceSource::CveHost => "Host CVE",
        AcceptanceSource::CveEnvironment => "Environment CVE",
    };
    let today = Utc::now().date_naive();
    let status = acceptance_status(&entry);
    let status_class = acceptance_status_class(status);
    let review_deadline = review_deadline(&entry);
    let review_expired = review_is_expired(&entry, today);
    let review_label = review_deadline
        .map(display_acceptance_date)
        .unwrap_or_else(|| "Not set".into());
    let approved_by = approver_label(entry.accepted_by, catalog.as_ref());
    let approved_at = entry
        .accepted_at
        .map(display_acceptance_timestamp)
        .unwrap_or_else(|| "Approval not recorded".into());
    let drawer_subject = acceptance_drawer_subject(&entry);
    let human_id = entry.human_id.clone();
    let source = entry.source_id;
    let scope_label = entry
        .system_hostname
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            entry
                .environment_id
                .map(|id| environment_name(id, &environments))
        })
        .unwrap_or_else(|| "Scope unavailable".into());
    let replacement = entry.replacement_poam_id;
    let conversion_owner = if entry.source == AcceptanceSource::PolicyWaiver {
        owner()
    } else {
        assignee()
    };
    let entry_renew = entry.clone();
    let entry_convert = entry.clone();
    rsx! {
        div { class: "poam-tray-backdrop", onclick: move |_| if !busy() { on_close.call(()); } }
        aside {
            class: "fl-tray poam-tray rr-acceptance-drawer",
            role: "dialog",
            aria_modal: "true",
            aria_label: "Risk acceptance {human_id} · {label}",
            tabindex: "-1",
            onclick: move |event| event.stop_propagation(),
            header { class: "fl-tray-head rr-acceptance-head",
                div { class: "rr-acceptance-title",
                    Icon { name: IconName::Shield, size: 18 }
                    div { class: "rr-acceptance-title-copy",
                        div { class: "rr-acceptance-title-line",
                            h2 { "{human_id} · {label}" }
                            span { class: "chip {status_class}", "{status}" }
                            if review_expired { span { class: "chip chip-critical", "review expired" } }
                        }
                        p { "{drawer_subject}" }
                    }
                }
                button { r#type: "button", class: "btn-icon focus-ring", aria_label: "Close acceptance", disabled: busy(), onclick: move |_| on_close.call(()), Icon { name: IconName::X, size: 16 } }
            }
            div { class: "rr-acceptance-scroll",
                div { class: "rr-acceptance-meta",
                    div { span { "Approved by" } strong { "{approved_by}" } }
                    div { span { "Approved" } strong { "{approved_at}" } }
                    div { span { "Review by" } strong { class: if review_expired { "poam-overdue" } else { "" }, "{review_label}" }
                        if review_expired { small { class: "rr-acceptance-expired", "Review deadline passed" } }
                        if review_deadline.is_none() { small { "No review date recorded" } }
                    }
                    div { span { "Scope" } strong { "{scope_label}" }
                        if let Some(expiry) = entry.expires_at {
                            small { "Policy authorization expires {display_acceptance_timestamp(expiry)}" }
                        }
                    }
                }
                details { class: "rr-acceptance-source-id",
                    summary { "Source record identity" }
                    code { "Decision {source}" }
                    if let Some(id) = replacement { code { "Replacement POA&M {id}" } }
                    if let Some(finding_id) = entry.finding_id { code { "Policy finding {finding_id}" } }
                    if let Some(version_id) = entry.policy_version_id { code { "Policy version {version_id}" } }
                }
                section { class: "rr-acceptance-section",
                    h3 { "Justification" }
                    p { "{entry.justification}" }
                }
                section { class: "rr-acceptance-section",
                    header { h3 { "Decision scope and evidence" } }
                    table { class: "sys-table compact sys-table-dense rr-acceptance-evidence",
                        thead { tr { th { "Host / scope" } th { "Finding" } th { "Package / policy" } } }
                        tbody { tr {
                            td { class: "mono", "{scope_label}" }
                            td { class: "mono", if let Some(cve) = entry.canonical_cve_id.as_deref() { "{cve}" } else if entry.finding_id.is_some() { "Policy finding" } else { "Not recorded" } }
                            td { class: "mono", if let Some(package) = entry.canonical_package_name.as_deref() { "{package}" } else if entry.policy_version_id.is_some() { "Policy waiver" } else { "Not recorded" } }
                        } }
                    }
                    p { class: "rr-acceptance-help", Icon { name: IconName::Shield, size: 12 } "Risk acceptance records a decision. It does not make a finding pass or mark it remediated." }
                    if let Some(reason) = entry.retirement_reason.as_deref() {
                        p { class: "rr-acceptance-retirement", "Retirement reason: {reason}" }
                    }
                    if let Some(id) = replacement {
                        button { class: "poam-ref focus-ring", onclick: move |_| {
                            on_close.call(());
                            nav.push(Route::PoamsView { query: RegisterLocation { poam: Some(id), ..RegisterLocation::parse("") }.query() });
                        }, Icon { name: IconName::Activity, size: 12 } " Open replacement POA&M " Icon { name: IconName::ArrowRight, size: 11 } }
                    }
                }
                if converting() && can_convert {
                    section { aria_label: "Conversion metadata",
                        h3 { "Convert to POA&M" }
                        p { "The source service verifies the original finding. Conversion does not change technical evidence." }
                        label { "Title" input { aria_label: "Conversion title", class: "input", value: "{title()}", oninput: move |e| { title.set(e.value()); confirming.set(false); } } }
                        label { "Remediation plan" textarea { aria_label: "Conversion plan", class: "input", value: "{plan()}", oninput: move |e| { plan.set(e.value()); confirming.set(false); } } }
                        if entry.source == AcceptanceSource::PolicyWaiver { label { "Owner" input { aria_label: "Conversion owner", class: "input", value: "{owner()}", oninput: move |e| { owner.set(e.value()); confirming.set(false); } } } }
                        label { "Assignee" select { aria_label: "Conversion assignee", class: "cfgx-select", value: "{assignee()}", onchange: move |e| { assignee.set(e.value()); confirming.set(false); },
                            option { value: "", "Select assignee" }
                            if entry.source == AcceptanceSource::PolicyWaiver { option { value: "unassigned", "Explicitly unassigned" } }
                            if let Some(c) = &catalog { for person in &c.people { option { value: "user:{person.user_id}", "{person.label}" } } for group in &c.groups { option { value: "group:{group.group_name}", "Group: {group.group_name}" } } }
                        } }
                        label { "Risk" select { aria_label: "Conversion risk", class: "cfgx-select", value: "{risk()}", onchange: move |e| { risk.set(e.value()); confirming.set(false); }, option { value: "", "Select risk" } option { value: "high", "CAT I" } option { value: "medium", "CAT II" } option { value: "low", "CAT III" } } }
                        label { "Target date" input { r#type: "date", aria_label: "Conversion due date", class: "input", value: "{due()}", oninput: move |e| { due.set(e.value()); confirming.set(false); } } }
                        if entry.source == AcceptanceSource::PolicyWaiver { label { "Reuse compatible plan UUID (optional)" input { aria_label: "Reuse plan UUID", class: "input", value: "{reuse()}", oninput: move |e| { reuse.set(e.value()); confirming.set(false); } } } }
                        p { role: "status", "{validation}" }
                    }
                    if confirming() && valid { section { aria_label: "Confirm conversion metadata",
                        h3 { "Confirm replacement" }
                        p { "{title()} · {plan()} · {risk()} · due {due()}" }
                        p { "Owner: {conversion_owner}" }
                        if let Some(id) = reuse_id.flatten() { p { "Reuse compatible plan {id}" } }
                    } }
                }
                if let Some(reason) = error() { p { role: "alert", "{human_id}: {reason}" } }
            }
            footer { class: "rr-tray-foot rr-acceptance-footer",
                span { class: "rr-acceptance-footer-note", if can_renew || can_convert { "Source-owned actions · original decision remains in history" } else { "Read-only decision record" } }
                div { class: "rr-acceptance-footer-actions",
                if can_renew { button { r#type: "button", class: "btn btn-ghost focus-ring", disabled: busy(), onclick: move |_| { busy.set(true); error.set(None); let entry = entry_renew.clone(); spawn(async move {
                    match poam_api::renew_acceptance(&entry).await { Ok(_) => on_changed.call(()), Err(err) => error.set(Some(err.to_string())) }
                    busy.set(false);
                }); }, Icon { name: IconName::Clock, size: 13 } " Re-review · renew 90 days" } }
                if can_convert && !converting() { button { r#type: "button", class: "btn btn-primary focus-ring", disabled: busy(), onclick: move |_| converting.set(true), Icon { name: IconName::Plus, size: 13 } " Convert to POA&M" } }
                if converting() { button { r#type: "button", class: "btn btn-ghost focus-ring", disabled: busy(), onclick: move |_| { converting.set(false); confirming.set(false); }, "Cancel" }
                    if !confirming() { button { r#type: "button", class: "btn btn-primary focus-ring", disabled: !valid || busy(), onclick: move |_| confirming.set(true), "Review conversion" } }
                    else { button { r#type: "button", class: "btn btn-primary focus-ring", disabled: !valid || busy(), onclick: move |_| {
                        let Some((date, risk, reuse_id)) = date.zip(parsed_risk).zip(reuse_id).map(|((d, r), id)| (d, r, id)) else { return; };
                        let title = title().trim().to_string(); let plan = plan().trim().to_string(); let owner = owner().trim().to_string();
                        let assignee = typed.clone(); let entry = entry_convert.clone();
                        busy.set(true); error.set(None);
                        spawn(async move {
                            let result = match entry.source {
                                AcceptanceSource::PolicyWaiver => poam_api::convert_acceptance(&entry, &CreatePoamRequest {
                                    assessment_id: None, finding_id: None, observation: None, title, plan, owner,
                                    assignee, target_date: Some(date), risk, default_milestones: true, assignment_version_ids: Vec::new(),
                                }, reuse_id).await,
                                _ => match assignee {
                                    Some(assignee @ (PoamAssigneeRequest::User { .. } | PoamAssigneeRequest::OidcGroup { .. })) => {
                                        poam_api::convert_acceptance(&entry, &FleetCvePoamRequest {
                                            title, plan, assignee, target_date: date,
                                            risk, default_milestones: true,
                                        }, None).await
                                    }
                                    _ => Err(PoamApiError::Deserialize("Choose a user or group assignee for CVE remediation".into())),
                                },
                            };
                            match result { Ok(_) => on_changed.call(()), Err(err) => error.set(Some(err.to_string())) }
                            busy.set(false);
                        });
                    }, "Confirm conversion" } }
                }
                }
            }
        }
    }
}

#[component]
fn AcceptanceRegister(
    location: RegisterLocation,
    refresh: Signal<u64>,
    catalog: Option<PoamAssigneeCatalog>,
    overview: Signal<Vec<AcceptanceEntry>>,
    environments: Vec<EnvironmentSummary>,
    group_by: String,
    sort_by: String,
    status_filter: String,
    risk_filter: String,
    mine: bool,
    me: Option<Uuid>,
    search: String,
    progress_column: bool,
    owner_column: bool,
) -> Element {
    let nav = use_navigator();
    let app = use_context::<Signal<AppState>>();
    let auth_context = app.read().auth.clone();
    let operator = auth::is_operator_or_above(&auth_context);
    let admin = auth::is_admin(&auth_context);
    let mut rows = use_signal(Vec::<AcceptanceEntry>::new);
    let mut total = use_signal(|| 0_i64);
    let mut has_more = use_signal(|| false);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut loaded_scope = use_signal(String::new);
    let mut loaded_search = use_signal(String::new);
    let mut selected = use_signal(BTreeSet::<AcceptanceId>::new);
    let mut collapsed = use_signal(BTreeSet::<String>::new);
    let mut opened = use_signal(|| None::<AcceptanceId>);
    let mut busy = use_signal(|| false);
    let mut outcome = use_signal(|| None::<String>);
    let environment_id = match location.scope {
        Some(Scope::Environment(id)) => Some(id),
        _ => None,
    };
    let scope_key = location.selection_key();
    use_effect(use_reactive(
        &(scope_key.clone(), refresh(), search.clone()),
        move |(key, generation, query)| {
            let changed_scope = *loaded_scope.peek() != key || *loaded_search.peek() != query;
            loaded_scope.set(key.clone());
            loaded_search.set(query.clone());
            if changed_scope {
                rows.set(Vec::new());
                overview.set(Vec::new());
                total.set(0);
                has_more.set(false);
                selected.set(BTreeSet::new());
                opened.set(None);
            }
            error.set(None);
            loading.set(true);
            spawn(async move {
                let result = refresh_acceptances(
                    if changed_scope { 0 } else { rows.peek().len() },
                    environment_id,
                    &query,
                )
                .await;
                if *loaded_scope.peek() != key
                    || *loaded_search.peek() != query
                    || *refresh.peek() != generation
                {
                    return;
                }
                match result {
                    Ok((items, count, more)) => {
                        total.set(count);
                        has_more.set(more);
                        overview.set(items.clone());
                        rows.set(items);
                    }
                    Err(err) => error.set(Some(err.to_string())),
                }
                loading.set(false);
            });
        },
    ));
    let loaded = rows();
    let today = Utc::now().date_naive();
    let mut shown: Vec<_> = loaded
        .iter()
        .filter(|item| {
            acceptance_matches(
                item,
                location,
                today,
                &status_filter,
                &risk_filter,
                mine,
                me,
                &search,
                &environments,
            )
        })
        .cloned()
        .collect();
    shown.sort_by(|a, b| acceptance_compare(a, b, &sort_by));
    let mut groups: BTreeMap<String, Vec<AcceptanceEntry>> = BTreeMap::new();
    for item in shown {
        for key in acceptance_group_keys(&item, &group_by, &environments, catalog.as_ref(), today) {
            groups.entry(key).or_default().push(item.clone());
        }
    }
    let chosen = selected();
    let batch: Vec<_> = loaded
        .iter()
        .filter(|item| chosen.contains(&acceptance_id(item)))
        .cloned()
        .collect();
    let eligible: Vec<_> = batch
        .iter()
        .filter(|item| renewable(item, operator, admin))
        .cloned()
        .collect();
    let opened_entry = opened().and_then(|id| {
        loaded
            .iter()
            .find(|item| acceptance_id(item) == id)
            .cloned()
    });
    rsx! {
        div { class: "poams-acceptances", aria_label: "Risk acceptances",
            if loading() && loaded.is_empty() { p { class: "poams-notice", role: "status", "Loading accepted decisions..." } }
            if let Some(message) = error() { p { class: "poams-notice", role: "alert", "Could not load risk acceptances: {message}" } }
            if !loading() && groups.is_empty() && error().is_none() { p { class: "poams-notice", "No decisions match these filters." } }
            for (group_name, items) in groups {
                { let decisions: Vec<_> = items.iter().collect();
                  let distribution = group_distribution(&[], &decisions);
                  let display_name = system_group_name(&group_name, &[], &decisions);
                  let focus = acceptance_group_focus(&group_by, &items);
                  let color = match focus { Some(Scope::Environment(id)) => environment_color(id, &environments), _ => None };
                  let is_collapsed = collapsed().contains(&group_name);
                  let expired = items.iter().filter(|item| acceptance_queue(item, Some(Queue::ExpiredAcceptance), today)).count();
                  let soon = items.iter().filter(|item| acceptance_queue(item, Some(Queue::ReviewSoon), today)).count();
                  rsx! { section { class: "poams-group pv-group", key: "acceptance-{group_name}",
                    if group_by != "none" { div { class: "pv-group-head rr-acceptance-group-head", role: "group", aria_label: "Acceptance group {display_name}",
                        button { r#type: "button", class: "poams-group-toggle pv-group-toggle focus-ring", aria_expanded: if is_collapsed { "false" } else { "true" }, onclick: { let key = group_name.clone(); move |_| { let mut set = collapsed(); if !set.insert(key.clone()) { set.remove(&key); } collapsed.set(set); selected.set(BTreeSet::new()); } },
                            Icon { name: if is_collapsed { IconName::ChevronRight } else { IconName::ChevronDown }, size: 12 }
                            if let Some(color) = color { span { class: "rr-group-source-dot", style: "background:{color};", aria_hidden: "true" } }
                            else if group_name == "Host-only decisions" { span { class: "rr-group-neutral-dot", aria_hidden: "true" } }
                            span { class: "pv-group-name", "{display_name}" } span { class: "pv-group-n", "{items.len()}" }
                        }
                        if expired > 0 { span { class: "pv-group-late", "{expired} expired" } }
                        if soon > 0 { span { class: "rr-group-review-soon", "{soon} review soon" } }
                        span { class: "pv-stack", aria_label: "Acceptance status distribution", for (label, color, count) in distribution { span { title: "{count} {label}", style: "flex:{count};background:{color};" } } }
                        if let Some(scope) = focus { button { r#type: "button", class: "btn btn-ghost xs focus-ring pv-group-focus", onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { scope: Some(scope), poam: None, ..location }.query() }); }, "Focus" } }
                    } }
                if !is_collapsed {
                div { class: "poams-table-wrap pv-table-wrap",
                    table { class: "sys-table compact sys-table-dense poams-table pv-table rr-acceptances-table rr-acceptances-only",
                        thead { tr { th { "ID" } th { "Title" } th { "Status" } th { class: if progress_column { "pv-c-ms rr-appr" } else { "pv-c-ms rr-appr poams-hidden" }, "Approved" } th { class: if owner_column { "pv-c-owner" } else { "pv-c-owner poams-hidden" }, "Approver" } th { "Review" } } }
                        tbody { for item in items {
                             tr { key: "{item.source:?}:{item.source_id}", "data-source-id": "{item.source_id}", class: if chosen.contains(&acceptance_id(&item)) { "selectable row-checked" } else { "selectable" },
                                aria_selected: if chosen.contains(&acceptance_id(&item)) { "true" } else { "false" }, tabindex: "0",
                                onclick: { let item = item.clone(); move |e: MouseEvent| {
                                    let id = acceptance_id(&item);
                                    if e.modifiers().ctrl() || e.modifiers().meta() {
                                        let mut ids = selected(); if !ids.insert(id) { ids.remove(&id); } selected.set(ids);
                                    } else { opened.set(Some(id)); }
                                } },
                                onkeydown: { let item = item.clone(); move |e: KeyboardEvent| if e.key() == Key::Enter { opened.set(Some(acceptance_id(&item))); } },
                                 td { class: "mono", "{item.human_id}" }
                                 td { div { class: "pv-title", "{acceptance_row_title(&item, &environments)}" }
                                     div { class: "pv-sub", span { class: "rr-source-type", "{acceptance_source_label(item.source)}" } span { class: "rr-just", "{item.justification}" } }
                                }
                                td { span { class: "chip {acceptance_status_class(acceptance_status(&item))}", "{acceptance_status(&item)}" } }
                                td { class: if progress_column { "pv-c-ms rr-appr" } else { "pv-c-ms poams-hidden" }, if let Some(at) = item.accepted_at { "{display_acceptance_timestamp(at)}" } else { "Approval not recorded" } }
                                td { class: if owner_column { "pv-c-owner" } else { "pv-c-owner poams-hidden" }, "{approver_label(item.accepted_by, catalog.as_ref())}" }
                                td { class: if review_is_expired(&item, today) { "rr-review-cell expired" } else { "rr-review-cell" },
                                    if let Some(date) = review_deadline(&item) { span { class: "rr-review-date", "{display_acceptance_date(date)}" } small { class: "rr-review-relative", "{acceptance_review_relative(&item, today)}" } } else { "No review date" }
                                    if let Some(expiry) = item.expires_at { small { class: "rr-authorization-expiry", "Authorization expires {display_acceptance_timestamp(expiry)}" } }
                                }
                            }
                        } }
                    }
                } }
                }
                }
            }
            }
            if has_more() { button { r#type: "button", class: "btn btn-ghost focus-ring poams-more", disabled: loading(), onclick: move |_| {
                loading.set(true);
                let offset = rows().len() as i64;
                let key = location.selection_key();
                let query = search.clone();
                spawn(async move {
                    let result = poam_api::list_acceptances(offset, environment_id, &query).await;
                    if *loaded_scope.peek() != key || *loaded_search.peek() != query { return; }
                    match result {
                        Ok(page) => { rows.write().extend(page.items); overview.set(rows()); total.set(page.total); has_more.set(page.has_more); error.set(None); }
                        Err(err) => error.set(Some(err.to_string())),
                    }
                    loading.set(false);
                });
            }, "Load more accepted decisions" } }
            if !chosen.is_empty() { div { class: "bulk-bar poams-bulk", role: "group", aria_label: "Selected risk acceptances",
                span { "{chosen.len()} source decisions selected" }
                button { r#type: "button", class: "btn btn-ghost xs focus-ring", disabled: busy() || chosen.len() > 100 || eligible.is_empty() || loading(), onclick: move |_| {
                    let targets = eligible.clone(); let selected_before = selected(); let scope = location.selection_key();
                    busy.set(true); outcome.set(None);
                    spawn(async move {
                        let mut succeeded = BTreeSet::new(); let mut failures = Vec::new();
                        for item in &targets {
                            let result = poam_api::renew_acceptance(item).await;
                            if *loaded_scope.peek() != scope { busy.set(false); return; }
                            match result { Ok(_) => { succeeded.insert(acceptance_id(item)); }, Err(err) => failures.push(format!("{} {}: {err}", item.source.key(), item.source_id)) }
                        }
                        selected.set(selected_before.difference(&succeeded).copied().collect());
                        let ineligible = selected_before.len() - targets.len();
                        outcome.set(Some(format!("{} renewed; {} failed; {} ineligible. Failed and ineligible IDs remain selected. {}", succeeded.len(), failures.len(), ineligible, failures.join("; "))));
                        if !succeeded.is_empty() { refresh.set(refresh().wrapping_add(1)); }
                        busy.set(false);
                    });
                }, "Renew {eligible.len()} acceptance(s) 90 days" }
                button { r#type: "button", class: "btn btn-ghost xs focus-ring", disabled: busy(), onclick: move |_| { selected.set(BTreeSet::new()); outcome.set(None); }, "Clear" }
                if chosen.len() > 100 { span { "Select at most 100 source identities." } }
                if eligible.len() != batch.len() { span { "{batch.len() - eligible.len()} ineligible loaded decisions will not be renewed." } }
            } }
            if let Some(result) = outcome() { p { role: "status", "{result}" } }
            if let Some(entry) = opened_entry { AcceptanceTray { key: "{entry.source:?}:{entry.source_id}", entry, environments, operator, admin, catalog,
                on_close: move |_| opened.set(None), on_changed: move |_| { refresh.set(refresh().wrapping_add(1)); }
            } }
        }
    }
}

/// Shows an authenticated, progressively loaded remediation register.
///
/// A POA&M detail URL retains the existing detail tray, while old
/// `/compliance?poam=` URLs continue to resolve through ComplianceView.
#[component]
pub fn PoamsView(query: String) -> Element {
    let location = RegisterLocation::parse(&query);
    rsx! { PoamsRegister { key: "{location.selection_key()}", location } }
}

#[component]
fn PoamsRegister(location: RegisterLocation) -> Element {
    let app = use_context::<Signal<AppState>>();
    let auth_context = app.read().auth.clone();
    let viewer = !auth::is_operator_or_above(&auth_context);
    let is_admin = auth::is_admin(&auth_context);
    let me = auth_context
        .as_ref()
        .and_then(|ctx| ctx.user.as_ref())
        .and_then(|user| Uuid::parse_str(&user.id).ok());
    let mut rows = use_signal(Vec::<PoamRegisterSummary>::new);
    let mut next = use_signal(|| None::<i64>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut overflow_open = use_signal(|| false);
    let mut overflow_search = use_signal(String::new);
    let mut status = use_signal(|| "active".to_string());
    let mut risk = use_signal(|| "all".to_string());
    let mut mine = use_signal(|| false);
    let mut search = use_signal(String::new);
    let mut grouping = use_signal(|| "environment".to_string());
    let mut sort = use_signal(|| "urgency".to_string());
    let mut progress_column = use_signal(|| true);
    let mut owner_column = use_signal(|| true);
    let mut visible = use_signal(|| 50usize);
    let mut collapsed = use_signal(BTreeSet::<String>::new);
    let mut selected = use_signal(BTreeSet::<Uuid>::new);
    let mut anchor = use_signal(|| None::<Uuid>);
    let mut catalog = use_signal(|| None::<PoamAssigneeCatalog>);
    let mut catalog_error = use_signal(|| None::<String>);
    let mut bulk_busy = use_signal(|| false);
    let mut bulk_outcome = use_signal(|| None::<BulkOutcome>);
    let mut refresh_error = use_signal(|| None::<String>);
    let mut loaded_scope = use_signal(String::new);
    let mut refresh = use_signal(|| 0_u64);
    let mut acceptance_overview = use_signal(Vec::<AcceptanceEntry>::new);
    let mut acceptance_next = use_signal(|| false);
    let mut acceptance_loading = use_signal(|| true);
    let mut acceptance_error = use_signal(|| None::<String>);
    let mut acceptance_scope = use_signal(String::new);
    let mut acceptance_search = use_signal(String::new);
    let mut mixed_open = use_signal(|| None::<AcceptanceId>);
    let mut mixed_selected = use_signal(BTreeSet::<AcceptanceId>::new);
    let mut mixed_busy = use_signal(|| false);
    let mut mixed_outcome = use_signal(|| None::<String>);
    let mut environments = use_signal(Vec::<EnvironmentSummary>::new);
    let mut export_open = use_signal(|| false);
    let mut columns_open = use_signal(|| false);
    let nav = use_navigator();
    let detail_id = location.poam;

    // CONCURRENCY: A URL scope change can finish after a previous list request.
    // Discard that response so it cannot replace the new scope's rows.
    let scope_key = location.selection_key();
    use_effect(use_reactive(
        &(scope_key.clone(), refresh()),
        move |(key, generation)| {
            let changed_scope = *loaded_scope.peek() != key;
            loaded_scope.set(key.clone());
            if changed_scope {
                rows.set(Vec::new());
                next.set(None);
                selected.write().clear();
                anchor.set(None);
            }
            error.set(None);
            loading.set(true);
            spawn(async move {
                let result = refresh_loaded_pages(
                    location,
                    if changed_scope { 0 } else { rows.peek().len() },
                )
                .await;
                if *loaded_scope.peek() != key || *refresh.peek() != generation {
                    return;
                }
                match result {
                    Ok((items, cursor)) => {
                        rows.set(items);
                        next.set(cursor);
                    }
                    Err(err) => error.set(Some(err.to_string())),
                }
                loading.set(false);
            });
        },
    ));
    use_future(move || async move {
        if !viewer {
            match poam_api::fetch_assignee_catalog().await {
                Ok(data) => catalog.set(Some(data)),
                Err(error) => catalog_error.set(Some(error.to_string())),
            }
        }
    });
    use_future(move || async move {
        if let Ok(items) = fetch_environments().await {
            environments.set(items);
        }
    });
    // CONCURRENCY: The two source readers page independently. A response for an
    // old URL scope or refresh generation must not replace current decisions.
    use_effect(use_reactive(
        &(scope_key, refresh(), search()),
        move |(key, generation, query)| {
            if location.kind == Tab::Acceptances {
                return;
            }
            let changed = *acceptance_scope.peek() != key || *acceptance_search.peek() != query;
            acceptance_scope.set(key.clone());
            acceptance_search.set(query.clone());
            if changed {
                acceptance_overview.set(Vec::new());
                acceptance_next.set(false);
                mixed_selected.set(BTreeSet::new());
                mixed_open.set(None);
            }
            acceptance_loading.set(true);
            acceptance_error.set(None);
            spawn(async move {
                let environment_id = match location.scope {
                    Some(Scope::Environment(id)) => Some(id),
                    _ => None,
                };
                match refresh_acceptances(
                    if changed {
                        0
                    } else {
                        acceptance_overview.peek().len()
                    },
                    environment_id,
                    &query,
                )
                .await
                {
                    Ok((items, _, more))
                        if *acceptance_scope.peek() == key
                            && *acceptance_search.peek() == query
                            && *refresh.peek() == generation =>
                    {
                        acceptance_overview.set(items);
                        acceptance_next.set(more);
                    }
                    Err(err)
                        if *acceptance_scope.peek() == key
                            && *acceptance_search.peek() == query =>
                    {
                        acceptance_error.set(Some(err.to_string()))
                    }
                    _ => return,
                }
                acceptance_loading.set(false);
            });
        },
    ));

    let run_bulk = move |action: BulkAction| {
        if viewer || bulk_busy() || refresh_error().is_some() || loading() {
            return;
        }
        let original = selected();
        let before = rows();
        let Some(targets) = batch_targets(&before, &original, &action) else {
            return;
        };
        bulk_busy.set(true);
        bulk_outcome.set(None);
        let request_scope = location.selection_key();
        spawn(async move {
            let outcome = apply_bulk_action(&targets, &action).await;
            // CONCURRENCY: A server mutation may finish after navigation. Do not
            // apply its selection or page refresh to a different register scope.
            if *loaded_scope.peek() != request_scope {
                bulk_busy.set(false);
                return;
            }
            selected.set(remaining_selection(&original, &outcome));
            anchor.set(None);
            // Do not combine stale page summaries with the new revisions. If
            // refresh fails, keep selection but block further bulk mutations.
            let refreshed = refresh_loaded_pages(location, before.len()).await;
            if *loaded_scope.peek() != request_scope {
                bulk_busy.set(false);
                return;
            }
            match refreshed {
                Ok((fresh, cursor)) => {
                    rows.set(fresh);
                    next.set(cursor);
                    refresh_error.set(None);
                }
                Err(err) => refresh_error.set(Some(err.to_string())),
            }
            bulk_outcome.set(Some(outcome));
            bulk_busy.set(false);
        });
    };

    let today = Utc::now().date_naive();
    let loaded = rows();
    let has_more = next().is_some();
    // The server has no equivalent of the loaded-only queue/search or Mine
    // filter. It does support active/completed status and risk; never offer a
    // file as if it represented unsupported client-side selections.
    let export_plans = location.scope.is_none()
        && location.queue.is_none()
        && !mine()
        && search().trim().is_empty();
    let server_status = match status().as_str() {
        "active" => Some("active"),
        "closed" => Some("completed"),
        _ => None,
    };
    // A plan-risk filter excludes decisions because acceptance sources have no
    // plan-risk category. Select only source families present in the view.
    let export_family = match location.kind {
        Tab::Acceptances => "acceptances",
        Tab::Plans => "plans",
        Tab::Everything if risk() != "all" => "plans",
        Tab::Everything => "all",
    };
    let plan_filters = if export_family == "acceptances" {
        String::new()
    } else {
        format!(
            "{}{}",
            server_status
                .map(|value| format!("&poam_status={value}"))
                .unwrap_or_default(),
            if risk() == "all" {
                String::new()
            } else {
                format!("&poam_risk={}", risk())
            }
        )
    };
    let acceptance_filters = if export_family == "plans" {
        String::new()
    } else if status() == "all" {
        "&acceptance_status=accepted_or_converted".into()
    } else {
        "&acceptance_status=accepted_current".into()
    };
    let mixed_export = format!(
        "{}/register/export?record_type={export_family}{plan_filters}{acceptance_filters}&format=",
        crate::api::client::base_url()
    );
    let acceptance_rows = acceptance_overview();
    // A decision has no plan-risk category or inferred host/environment edge.
    // Count only the source identities that the active scope can prove.
    let scoped_acceptances: Vec<_> = acceptance_rows
        .iter()
        .filter(|item| acceptance_in_scope(item, location.scope))
        .collect();
    let acceptance_visible: Vec<_> = acceptance_rows
        .iter()
        .filter(|item| {
            acceptance_matches(
                item,
                location,
                today,
                &status(),
                &risk(),
                mine(),
                me,
                &search(),
                &environments(),
            )
        })
        .collect();
    let mixed_eligible_count = acceptance_rows
        .iter()
        .filter(|item| {
            mixed_selected().contains(&acceptance_id(item)) && renewable(item, !viewer, is_admin)
        })
        .count();
    let active_acceptances = scoped_acceptances
        .iter()
        .filter(|item| {
            item.status == "accepted"
                && item.retired_at.is_none()
                && item.replacement_poam_id.is_none()
        })
        .count();
    let expired_acceptances = scoped_acceptances
        .iter()
        .filter(|item| acceptance_queue(item, Some(Queue::ExpiredAcceptance), today))
        .count();
    let soon_acceptances = scoped_acceptances
        .iter()
        .filter(|item| acceptance_queue(item, Some(Queue::ReviewSoon), today))
        .count();
    let undated_acceptances = scoped_acceptances
        .iter()
        .filter(|item| acceptance_queue(item, Some(Queue::NoReview), today))
        .count();
    let env_names = environments();
    let scope_label = location.scope.map(|s| match s {
        Scope::Environment(id) => environment_name(id, &env_names),
        Scope::System(_) => "Host".into(),
        Scope::Bundle(id) => format!("Bundle {id}"),
        Scope::Owner(id) => approver_label(Some(id), catalog().as_ref()),
    });
    let available = loaded.len();
    let open = loaded
        .iter()
        .filter(|r| in_scope(r, location.scope) && r.summary.status.is_active())
        .count();
    let filtered: Vec<_> = loaded
        .iter()
        .filter(|row| {
            let p = &row.summary;
            (status() == "all" || (status() == "active") == p.status.is_active())
                && (risk() == "all" || risk() == p.risk.label().to_lowercase())
                && (!mine() || me.is_some_and(|id| in_scope(row, Some(Scope::Owner(id)))))
                && in_scope(row, location.scope)
                && location.queue.is_none_or(|q| q.includes(row, today))
                && (search().trim().is_empty()
                    || [
                        p.human_id.as_str(),
                        p.title.as_str(),
                        row.first_requirement.as_deref().unwrap_or(""),
                        row.first_cve.as_deref().unwrap_or(""),
                    ]
                    .iter()
                    .any(|v| v.to_lowercase().contains(&search().trim().to_lowercase())))
        })
        .collect();
    let mut ordered = filtered;
    ordered.sort_by(|a, b| {
        let x = &a.summary;
        let y = &b.summary;
        let risk_rank = |r: PoamRisk| match r {
            PoamRisk::High => 0,
            PoamRisk::Medium => 1,
            PoamRisk::Low => 2,
        };
        let cmp = match sort().as_str() {
            "due" => x.target_date.cmp(&y.target_date),
            "risk" => risk_rank(x.risk).cmp(&risk_rank(y.risk)),
            "activity" => b.last_activity_at.cmp(&a.last_activity_at),
            "id" => x.human_id.cmp(&y.human_id),
            _ => y
                .overdue
                .cmp(&x.overdue)
                .then_with(|| risk_rank(x.risk).cmp(&risk_rank(y.risk)))
                .then_with(|| x.target_date.cmp(&y.target_date)),
        };
        cmp.then_with(|| x.id.cmp(&y.id))
    });
    let matching = ordered.len();
    let export_count = match export_family {
        "plans" => matching,
        "acceptances" => acceptance_visible.len(),
        _ => matching + acceptance_visible.len(),
    };
    let export_may_have_more = (export_family != "acceptances" && has_more)
        || (export_family != "plans" && acceptance_next());
    let export_enabled = export_plans
        && (export_family == "plans" || status() != "closed")
        && (export_count > 0 || export_may_have_more);
    let export_label = format!(
        "Export {export_count}{}",
        if export_may_have_more { "+" } else { "" }
    );
    let mut groups: BTreeMap<String, Vec<&PoamRegisterSummary>> = BTreeMap::new();
    for row in ordered {
        let keys: Vec<String> = match grouping().as_str() {
            "environment" if matches!(location.scope, Some(Scope::System(_))) => vec!["Host scope".into()],
            "environment" => row
                .environment_ids
                .iter()
                .filter(|id| !matches!(location.scope, Some(Scope::Environment(selected)) if **id != selected))
                .map(|id| environment_name(*id, &env_names))
                .collect(),
            "system" => row.system_ids.iter()
                .filter(|id| !matches!(location.scope, Some(Scope::System(selected)) if **id != selected))
                .map(|id| format!("Host {id}")).collect(),
            "bundle" => row
                .bundle_ids
                .iter()
                .filter(|id| !matches!(location.scope, Some(Scope::Bundle(selected)) if **id != selected))
                .map(|id| format!("Bundle {id}"))
                .collect(),
            "owner" => vec![owner(row)],
            "type" => vec!["Remediation plans".into()],
            "due" => vec![if row.summary.status == PoamStatus::Completed {
                "Closed".into()
            } else if row.summary.overdue {
                "Late".into()
            } else {
                "Upcoming / no date".into()
            }],
            _ => vec!["Records".into()],
        };
        for key in if keys.is_empty() {
            vec!["No linked scope".into()]
        } else {
            keys
        } {
            groups.entry(key).or_default().push(row);
        }
    }
    let mut acceptance_groups: BTreeMap<String, Vec<&AcceptanceEntry>> = BTreeMap::new();
    if location.kind == Tab::Everything {
        for item in &acceptance_visible {
            for key in
                acceptance_group_keys(item, &grouping(), &env_names, catalog().as_ref(), today)
            {
                acceptance_groups.entry(key.clone()).or_default().push(item);
                groups.entry(key).or_default();
            }
        }
        for entries in acceptance_groups.values_mut() {
            entries.sort_by(|a, b| acceptance_compare(a, b, &sort()));
        }
    }
    let order = displayed_order(&groups, &collapsed(), visible());
    let current_selection = selected();
    let selection_count = current_selection.len();
    let can_assign = !viewer
        && !bulk_busy()
        && !loading()
        && refresh_error().is_none()
        && batch_targets(
            &loaded,
            &current_selection,
            &BulkAction::Assign(PoamAssigneeRequest::Unassigned),
        )
        .is_some();
    let can_extend = !viewer
        && !bulk_busy()
        && !loading()
        && refresh_error().is_none()
        && batch_targets(&loaded, &current_selection, &BulkAction::Extend).is_some();
    let legal_statuses: Vec<_> = [
        PoamStatus::Open,
        PoamStatus::InProgress,
        PoamStatus::Blocked,
        PoamStatus::AwaitingVerification,
    ]
    .into_iter()
    .filter(|status| {
        !viewer
            && !bulk_busy()
            && !loading()
            && refresh_error().is_none()
            && batch_targets(&loaded, &current_selection, &BulkAction::Status(*status)).is_some()
    })
    .collect();
    let has_legal_statuses = !legal_statuses.is_empty();
    let mut assign_bulk = run_bulk.clone();
    let mut status_bulk = run_bulk.clone();
    let mut extend_bulk = run_bulk;
    let shown_count = order.len();
    let visible_count = if location.kind == Tab::Acceptances {
        acceptance_visible.len()
    } else {
        shown_count
            + if location.kind == Tab::Everything {
                acceptance_visible.len().min(visible())
            } else {
                0
            }
    };
    let can_show_more = groups.values().any(|entries| entries.len() > visible());
    let pills = scope_pills(
        if location.kind == Tab::Acceptances {
            &[]
        } else {
            &loaded
        },
        location.dimension.key(),
        location.scope,
        &env_names,
        if location.kind == Tab::Plans {
            &[]
        } else {
            &acceptance_rows
        },
        catalog().as_ref(),
    );
    let host_only_acceptance_count = acceptance_visible
        .iter()
        .filter(|item| item.system_id.is_some() && item.environment_id.is_none())
        .count();
    let host_id = match location.scope {
        Some(Scope::System(id)) if loaded.iter().any(|row| row.system_ids.contains(&id)) => {
            Some(id)
        }
        _ => None,
    };
    // A flat register projection cannot prove a host-to-environment edge.
    let parent_env: Option<Uuid> = None;
    let row_view = |row: &PoamRegisterSummary, group_environment: Option<Uuid>| {
        let p = &row.summary;
        let id = p.id;
        let due = p
            .target_date
            .map(|date| date.to_string())
            .unwrap_or_else(|| "No date".into());
        let status_label = p.status.label();
        let risk_label = p.risk.category_label();
        let progress_width = if row.milestone_count == 0 {
            0
        } else {
            100 * row.completed_milestone_count / row.milestone_count
        };
        let owner_label = owner(row);
        let owner_id = match &p.assignee {
            Some(PoamAssigneeView::User { user_id, .. }) => Some(*user_id),
            _ => None,
        };
        let subtitle = plan_subtitle(
            row,
            group_environment,
            grouping() == "environment",
            match location.scope {
                Some(Scope::System(id)) => Some(id),
                _ => None,
            },
            &env_names,
        );
        let checked = selected().contains(&id);
        let order = order.clone();
        rsx! {
            tr { key: "{id}", class: if checked { "selectable row-checked" } else { "selectable" },
                tabindex: "0", aria_selected: if checked { "true" } else { "false" },
                onmousedown: move |event| if event.modifiers().shift() { event.prevent_default(); },
                onclick: move |event| {
                    if bulk_busy() { return; }
                    let shift = event.modifiers().shift(); let toggle = event.modifiers().ctrl() || event.modifiers().meta();
                    let mut ids = selected(); let mut at = anchor();
                    if select_row(&mut ids, &mut at, id, &order, shift, toggle) { event.prevent_default(); selected.set(ids); anchor.set(at); }
                    else { anchor.set(at); nav.push(Route::PoamsView { query: RegisterLocation { poam: Some(id), ..location }.query() }); }
                },
                onkeydown: move |event| if event.key() == Key::Enter { nav.push(Route::PoamsView { query: RegisterLocation { poam: Some(id), ..location }.query() }); },
                 td { class: "mono", "{p.human_id}" }
                   td { div { class: "pv-title", "{p.title}" }
                       div { class: "pv-sub", span { "{subtitle}" } }
                   }
                  td { class: "poams-risk", span { class: "chip {crate::components::poam::risk_class(p.risk)}", "{risk_label}" } }
                  td { span { class: "chip {crate::components::poam::status_class(p.status)}", "{status_label}" } }
                 td { class: if progress_column() { "pv-c-ms" } else { "pv-c-ms poams-hidden" }, div { class: "pv-prog", span { class: "pv-prog-bar", span { style: "width: {progress_width}%" } } span { class: "mono pv-prog-t", "{row.completed_milestone_count}/{row.milestone_count}" } } }
                 td { class: if owner_column() { "pv-c-owner" } else { "pv-c-owner poams-hidden" }, if let Some(user_id) = owner_id { button { r#type: "button", class: "pv-link focus-ring", onclick: move |e| { e.stop_propagation(); nav.push(Route::PoamsView { query: RegisterLocation { dimension: Dimension::Owner, scope: Some(Scope::Owner(user_id)), poam: None, ..location }.query() }); }, "{owner_label}" } } else { "{owner_label}" } }
                 td { class: if p.overdue { "poam-overdue" } else { "" }, "{due}" if p.overdue { small { "Overdue" } } }
            }
        }
    };
    let mixed_acceptance = |item: &AcceptanceEntry| {
        let id = acceptance_id(item);
        let checked = mixed_selected().contains(&id);
        let title = acceptance_row_title(item, &env_names);
        let source = acceptance_source_label(item.source);
        let approver = approver_label(item.accepted_by, catalog().as_ref());
        rsx! { tr { key: "{item.source:?}:{item.source_id}", "data-source-id": "{item.source_id}", class: if checked { "selectable row-checked" } else { "selectable" }, aria_selected: if checked { "true" } else { "false" }, tabindex: "0", onclick: move |e: MouseEvent| {
            if e.modifiers().ctrl() || e.modifiers().meta() {
                let mut ids = mixed_selected(); if !ids.insert(id) { ids.remove(&id); } mixed_selected.set(ids);
            } else { mixed_open.set(Some(id)); }
        }, onkeydown: move |e| if e.key() == Key::Enter { mixed_open.set(Some(id)); },
            td { class: "mono", "{item.human_id}" }
            td { div { class: "pv-title", "{title}" } div { class: "pv-sub", span { class: "rr-source-type", "{source}" } span { class: "rr-just", "{item.justification}" } } }
            td { span { class: "rr-no-risk", title: "Decision sources do not record a plan risk category", "No category" } }
            td { span { class: "chip {acceptance_status_class(acceptance_status(item))}", "{acceptance_status(item)}" } }
            td { class: if progress_column() { "pv-c-ms rr-appr" } else { "pv-c-ms poams-hidden" }, if let Some(at) = item.accepted_at { "{display_acceptance_timestamp(at)}" } else { "Approval not recorded" } }
            td { class: if owner_column() { "pv-c-owner" } else { "pv-c-owner poams-hidden" }, "{approver}" }
            td { class: if review_is_expired(item, today) { "rr-review-cell expired" } else { "rr-review-cell" },
                if let Some(date) = review_deadline(item) { span { class: "rr-review-date", "{display_acceptance_date(date)}" } small { class: "rr-review-relative", "{acceptance_review_relative(item, today)}" } } else { "No review date" }
                if let Some(expiry) = item.expires_at { small { class: "rr-authorization-expiry", "Authorization expires {display_acceptance_timestamp(expiry)}" } }
            }
        } }
    };
    let plan_tab_count = format!("{available}{}", if has_more { "+" } else { "" });
    let acceptance_tab_count = format!(
        "{}{}",
        scoped_acceptances.len(),
        if acceptance_next() { "+" } else { "" }
    );
    let everything_tab_count = format!(
        "{}{}",
        available + scoped_acceptances.len(),
        if has_more || acceptance_next() {
            "+"
        } else {
            ""
        }
    );

    rsx! {
        div { class: "poams-register",
            div { class: "page-head", div { h1 { class: "page-title", "POA&M" }
                p { class: "page-subtitle", "Showing {open} open remediation plans · {active_acceptances} active risk decisions" }
            }
                if export_enabled { div { class: "rr-export",
                    button { r#type: "button", class: "btn btn-ghost focus-ring", aria_expanded: if export_open() { "true" } else { "false" }, onclick: move |_| export_open.set(!export_open()), "{export_label} ▾" }
                    if export_open() { div { class: "rr-export-pop card", role: "menu", aria_label: "Export POA&Ms",
                        div { class: "rr-export-title", "Export POA&Ms" }
                        for (format, label, description) in [("oscal-json", "OSCAL JSON", "Machine-readable POA&M"), ("xlsx", "Excel XLSX", "Spreadsheet for review"), ("csv", "CSV", "Flat data for import"), ("oscal-xml", "OSCAL XML", "Standards-compatible XML")] {
                            a { role: "menuitem", class: "rr-export-item focus-ring", href: "{mixed_export}{format}", onclick: move |_| export_open.set(false), span { class: "rr-export-item-l", "{label}" } span { class: "rr-export-item-sub", "{description}" } }
                        }
                    } }
                } } else { button { r#type: "button", class: "btn btn-ghost focus-ring", disabled: true, title: if location.scope.is_some() || location.queue.is_some() || mine() || !search().trim().is_empty() { "Clear local filters to export the complete scope" } else if status() == "closed" && export_family != "plans" { "Choose All statuses to export historical decisions" } else { "No records to export" }, "Export ▾" } }
            }
            div { class: "rr-kinds", role: "tablist", aria_label: "Record type",
                for (kind, label, count) in [(Tab::Everything, "Everything", everything_tab_count), (Tab::Plans, "Remediation plans", plan_tab_count), (Tab::Acceptances, "Risk acceptances", acceptance_tab_count)] {
                    button { r#type: "button", role: "tab", class: if location.kind == kind { "rr-kind active focus-ring" } else { "rr-kind focus-ring" }, aria_selected: if location.kind == kind { "true" } else { "false" }, onclick: move |_| { export_open.set(false); columns_open.set(false); if kind == Tab::Acceptances { risk.set("all".into()); } nav.push(Route::PoamsView { query: RegisterLocation { kind, queue: None, poam: None, ..location }.query() }); },
                        if kind == Tab::Plans { span { class: "rr-kind-mark poams-plan-mark" } }
                        if kind == Tab::Acceptances { span { class: "rr-kind-mark poams-acceptance-mark" } }
                        "{label}" span { class: "rr-kind-n", "{count}" }
                    }
                }
                span { class: "rr-kinds-note", "Plans fix a deficiency by a target date; acceptances record a decision to let it stand until review." }
            }
            div { class: "pv-queues", role: "group", aria_label: "Work queues",
                if location.kind == Tab::Everything {
                    for (queue, count) in [
                        (Queue::Overdue, loaded.iter().filter(|row| in_scope(row, location.scope) && Queue::Overdue.includes(row, today)).count()),
                        (Queue::ExpiredAcceptance, expired_acceptances),
                        (Queue::ComingDue, loaded.iter().filter(|row| in_scope(row, location.scope) && Queue::ComingDue.includes(row, today)).count() + soon_acceptances),
                        (Queue::Awaiting, loaded.iter().filter(|row| in_scope(row, location.scope) && Queue::Awaiting.includes(row, today)).count()),
                        (Queue::Blocked, loaded.iter().filter(|row| in_scope(row, location.scope) && Queue::Blocked.includes(row, today)).count()),
                        (Queue::Gaps, loaded.iter().filter(|row| in_scope(row, location.scope) && Queue::Gaps.includes(row, today)).count() + undated_acceptances),
                    ] {
                        button { r#type: "button", class: if location.queue == Some(queue) { "pv-q active focus-ring" } else { "pv-q focus-ring" }, "data-queue": queue.key(), aria_pressed: if location.queue == Some(queue) { "true" } else { "false" }, onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { queue: if location.queue == Some(queue) { None } else { Some(queue) }, poam: None, ..location }.query() }); }, strong { class: "pv-q-count", "{count}" } span { class: "pv-q-label", "{queue.label()}" } span { class: "pv-q-sub", "{queue.description()}" } }
                    }
                }
                if location.kind == Tab::Plans {
                    for q in [Queue::Overdue, Queue::Soon, Queue::Awaiting, Queue::Blocked, Queue::Quiet, Queue::Unassigned].into_iter().filter(|q| location.kind != Tab::Everything || !matches!(q, Queue::Quiet | Queue::Unassigned)) {
                        button { r#type: "button", class: if location.queue == Some(q) { "pv-q active focus-ring" } else { "pv-q focus-ring" }, "data-queue": q.key(), aria_pressed: if location.queue == Some(q) { "true" } else { "false" }, onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { queue: if location.queue == Some(q) { None } else { Some(q) }, poam: None, ..location }.query() }); },
                            strong { class: "pv-q-count", "{loaded.iter().filter(|r| in_scope(r, location.scope) && q.includes(r, today)).count()}" } span { class: "pv-q-label", "{q.label()}" } span { class: "pv-q-sub", "{q.description()}" }
                        }
                    }
                }
                if location.kind == Tab::Acceptances {
                    for (queue, count, sub) in [(Queue::ExpiredAcceptance, expired_acceptances, "Renew or plan a fix"), (Queue::ReviewSoon, soon_acceptances, "Re-review before expiry"), (Queue::NoReview, undated_acceptances, "Assessors flag these"), (Queue::Accepted, active_acceptances, "Source-approved risk") ] {
                        button { r#type: "button", class: if location.queue == Some(queue) { "pv-q active focus-ring" } else { "pv-q focus-ring" }, "data-queue": queue.key(), aria_pressed: if location.queue == Some(queue) { "true" } else { "false" }, onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { kind: Tab::Acceptances, queue: if location.queue == Some(queue) { None } else { Some(queue) }, poam: None, ..location }.query() }); }, strong { class: "pv-q-count", "{count}" } span { class: "pv-q-label", "{queue.label()}" } span { class: "pv-q-sub", "{sub}" } }
                    }
                }
            }
            div { class: "card poams-main pv-main",
                    div { class: "poams-scope",
                        div { class: "seg xs", role: "tablist", aria_label: "Browse by",
                            for (key, label) in [("environment", "Environment"), ("bundle", "Bundle"), ("owner", "Owner / Approver")] {
                                button { r#type: "button", role: "tab", aria_selected: if location.dimension.key() == key { "true" } else { "false" }, class: if location.dimension.key() == key { "active" } else { "" }, onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { dimension: Dimension::parse(key), scope: None, poam: None, ..location }.query() }); }, "{label}" }
                            }
                        }
                        nav { aria_label: "Scope", class: "poams-crumb",
                            button { r#type: "button", class: "pv-link focus-ring", onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { scope: None, poam: None, ..location }.query() }); }, "All" }
                            if let Some(env) = parent_env { button { r#type: "button", class: "pv-link focus-ring", onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { scope: Some(Scope::Environment(env)), poam: None, ..location }.query() }); }, " / {environment_name(env, &env_names)}" } }
                            if let Some(label) = scope_label { strong { " / {label}" } }
                        }
                        span { "{visible_count} items" }
                        if let Some(host) = host_id { button { r#type: "button", class: "btn btn-ghost xs focus-ring", onclick: move |_| { nav.push(Route::SystemDetailView { id: host.to_string(), tab: "compliance".into(), poam: String::new(), config_mode: String::new(), revision: String::new(), generation: String::new(), deploy_generation: String::new(), cve_target: String::new(), cve_mode: String::new() }); }, "Open host" } }
                    }
                    if !pills.is_empty() || host_only_acceptance_count > 0 {
                        div { class: "poams-pills", aria_label: "Narrow to scopes",
                            for (key, label, count) in pills.iter().take(7).cloned() {
                                button { r#type: "button", class: "rr-pill focus-ring", onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { scope: Some(key), poam: None, ..location }.query() }); },
                                    if let Scope::Environment(id) = key { if let Some(color) = environment_color(id, &env_names) { span { class: "rr-scope-dot", style: "background:{color};", aria_hidden: "true" } } }
                                    span { "{label}" } span { "{count}" } }
                            }
                            if host_only_acceptance_count > 0 { span { class: "rr-host-only-summary", "Host-only {host_only_acceptance_count}" } }
                            if pills.len() > 7 { div { class: "poams-overflow",
                                button { r#type: "button", class: "rr-pill focus-ring", aria_expanded: if overflow_open() { "true" } else { "false" }, onclick: move |_| overflow_open.set(!overflow_open()), "+{pills.len() - 7} more" }
                                if overflow_open() { div { class: "card poams-picker", role: "dialog", aria_label: "More loaded scopes",
                                    input { class: "input focus-ring", aria_label: "Find loaded scope", placeholder: "Find UUID or owner", value: "{overflow_search()}", oninput: move |e| overflow_search.set(e.value()) }
                                    div { class: "poams-picker-list", for (key, label, count) in pills.iter().skip(7).filter(|(_, label, _)| label.to_lowercase().contains(&overflow_search().to_lowercase())).cloned() {
                                        button { r#type: "button", class: "rr-pill focus-ring", onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { scope: Some(key), poam: None, ..location }.query() }); }, "{label} ({count})" }
                                    } }
                                    button { r#type: "button", class: "btn btn-ghost xs focus-ring", onclick: move |_| overflow_open.set(false), "Close" }
                                } }
                            } }
                        }
                    }
                    div { class: "poams-toolbar pv-toolbar",
                        div { class: "rr-toolbar-primary",
                        input { class: "input focus-ring", aria_label: "Search register", placeholder: "Search ID, title, requirement, CVE", value: "{search()}", oninput: move |e| { search.set(e.value()); visible.set(50); selected.set(BTreeSet::new()); mixed_selected.set(BTreeSet::new()); anchor.set(None); } }
                        select { aria_label: "Status", class: "cfgx-select focus-ring", value: "{status()}", onchange: move |e| { status.set(e.value()); selected.set(BTreeSet::new()); mixed_selected.set(BTreeSet::new()); anchor.set(None); }, option { value: "active", "Active" } option { value: "closed", "Closed" } option { value: "all", "All" } }
                        select { aria_label: "Risk", class: "cfgx-select focus-ring", disabled: location.kind == Tab::Acceptances, title: if location.kind == Tab::Acceptances { "Decision sources do not record a plan risk category" } else { "Filter plan risk" }, value: "{risk()}", onchange: move |e| { risk.set(e.value()); selected.set(BTreeSet::new()); mixed_selected.set(BTreeSet::new()); anchor.set(None); }, option { value: "all", "Any risk" } option { value: "high", "CAT I" } option { value: "medium", "CAT II" } option { value: "low", "CAT III" } }
                        button { r#type: "button", class: "btn btn-ghost xs focus-ring", disabled: me.is_none(), aria_pressed: if mine() { "true" } else { "false" }, onclick: move |_| { mine.set(!mine()); selected.set(BTreeSet::new()); mixed_selected.set(BTreeSet::new()); anchor.set(None); }, "Mine" }
                        }
                        div { class: "rr-toolbar-secondary",
                        label { class: "pv-tool-label", "Group" }
                        select { aria_label: "Group register", class: "cfgx-select focus-ring", value: "{grouping()}", onchange: move |e| { grouping.set(e.value()); selected.set(BTreeSet::new()); mixed_selected.set(BTreeSet::new()); anchor.set(None); }, option { value: "none", "No grouping" } if location.kind == Tab::Everything { option { value: "type", "Type" } } option { value: "environment", "Environment" } option { value: "system", "Host" } option { value: "bundle", "Bundle" } option { value: "owner", "Owner / Approver" } option { value: "due", "Due / review date" } }
                        label { class: "pv-tool-label", "Sort" }
                        select { aria_label: "Sort register", class: "cfgx-select focus-ring", value: "{sort()}", onchange: move |e| { sort.set(e.value()); selected.set(BTreeSet::new()); mixed_selected.set(BTreeSet::new()); anchor.set(None); }, option { value: "urgency", "Urgency" } option { value: "due", "Due / review date" } if location.kind != Tab::Acceptances { option { value: "risk", "Risk" } } option { value: "activity", "Last activity" } option { value: "id", "ID" } }
                        div { class: "rr-cols", button { r#type: "button", class: "btn btn-ghost xs focus-ring", aria_expanded: if columns_open() { "true" } else { "false" }, onclick: move |_| columns_open.set(!columns_open()), "Columns" }
                            if columns_open() { div { class: "rr-cols-pop card", role: "menu", aria_label: "Toggle columns",
                                label { class: "rr-cols-item", input { r#type: "checkbox", checked: progress_column(), onchange: move |e| progress_column.set(e.checked()) } if location.kind == Tab::Acceptances { "Approved date" } else { "Progress" } }
                                label { class: "rr-cols-item", input { r#type: "checkbox", checked: owner_column(), onchange: move |e| owner_column.set(e.checked()) } "Owner" }
                            } }
                        }
                        span { class: "pv-selection-hint", "⌘ / ⌃ · click to select" }
                        }
                    }
                    if location.kind != Tab::Acceptances {
                    if loading() && available == 0 { div { class: "poams-notice", role: "status", "Loading plans..." } }
                    if let Some(err) = error() { div { class: "poams-notice", role: "alert", "Could not load plans: {err}" } }
                    if !loading() && matching == 0 && error().is_none() && location.kind == Tab::Plans { div { class: "poams-notice", "No plans match these filters." } }
                    for (name, entries) in groups {
                        {
                        let decision_rows = acceptance_groups.get(&name);
                        let decision_slice = decision_rows.map_or(&[][..], Vec::as_slice);
                        let distribution = group_distribution(&entries, decision_slice);
                        let display_name = system_group_name(&name, &entries, decision_slice);
                        let group_count = entries.len() + decision_rows.map_or(0, Vec::len);
                        let late_count = entries.iter().filter(|row| row.summary.overdue).count()
                            + decision_rows.map_or(0, |items| items.iter().filter(|item| acceptance_queue(item, Some(Queue::ExpiredAcceptance), today)).count());
                        let high_plans = entries.iter().filter(|row| row.summary.risk == PoamRisk::High).count();
                        let unique_environment = if grouping() == "environment" {
                            let matching: Vec<_> = env_names.iter().filter(|environment| environment.name == group_title(&name)).map(|environment| environment.id).collect();
                            if matching.len() == 1 { matching.first().copied() } else { None }
                        } else { None };
                        rsx! { section { class: "poams-group pv-group", key: "{name}", if grouping() != "none" { div { class: "pv-group-head", role: "group", aria_label: "Register group {display_name}", button { r#type: "button", class: "poams-group-toggle pv-group-toggle focus-ring", aria_expanded: if collapsed().contains(&name) { "false" } else { "true" }, onclick: { let name = name.clone(); move |_| { let mut set = collapsed(); if !set.insert(name.clone()) { set.remove(&name); } collapsed.set(set); selected.set(BTreeSet::new()); mixed_selected.set(BTreeSet::new()); anchor.set(None); } },
                            Icon { name: if collapsed().contains(&name) { IconName::ChevronRight } else { IconName::ChevronDown }, size: 12 }
                            if let Some(id) = unique_environment { if let Some(color) = environment_color(id, &env_names) { span { class: "rr-group-source-dot", style: "background:{color};", aria_hidden: "true" } } }
                            else if name == "Host-only decisions" { span { class: "rr-group-neutral-dot", aria_hidden: "true" } }
                            span { class: "pv-group-name", "{display_name}" } span { class: "pv-group-n", "{group_count}" } }
                            if late_count > 0 { span { class: "pv-group-late", "{late_count} late" } }
                            if high_plans > 0 { span { class: "pv-group-risk", "{high_plans} CAT I plans" } }
                            span { class: "pv-stack", aria_label: "Group status distribution",
                                for (label, color, count) in distribution { span { title: "{count} {label}", style: "flex:{count};background:{color};" } }
                            }
                            if let Some(environment) = unique_environment { button { r#type: "button", class: "btn btn-ghost xs focus-ring pv-group-focus", onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { scope: Some(Scope::Environment(environment)), poam: None, ..location }.query() }); }, "Focus" } }
                        } }
                            if !collapsed().contains(&name) {
                            div { class: "poams-table-wrap pv-table-wrap", table { class: "sys-table compact sys-table-dense poams-table pv-table", thead { tr { th { "ID" } th { "Title" } th { "Risk" } th { "Status" } th { class: if progress_column() { "pv-c-ms" } else { "pv-c-ms poams-hidden" }, "Progress" } th { class: if owner_column() { "pv-c-owner" } else { "pv-c-owner poams-hidden" }, "Owner" } th { "Due" } } }
                                tbody { for row in entries.into_iter().take(visible()) { {row_view(row, unique_environment)} }
                                    if location.kind == Tab::Everything {
                                        for item in acceptance_groups.get(&name).into_iter().flat_map(|items| items.iter().take(visible())) { {mixed_acceptance(item)} }
                                    }
                                }
                            } }
                            }
                        } }
                        }
                    }
                    if location.kind == Tab::Everything && acceptance_loading() && acceptance_rows.is_empty() { div { class: "poams-notice", role: "status", "Loading risk acceptances..." } }
                    if let Some(message) = acceptance_error() { div { class: "poams-notice", role: "alert", "Could not load risk acceptances: {message}" } }
                    if location.kind == Tab::Everything && acceptance_visible.len() > visible() { button { r#type: "button", class: "pv-more pv-more-row focus-ring", onclick: move |_| visible.set(visible() + 50), "Show more decisions" } }
                    if location.kind == Tab::Everything && acceptance_next() { button { r#type: "button", class: "btn btn-ghost focus-ring poams-more", disabled: acceptance_loading(), onclick: move |_| {
                        acceptance_loading.set(true);
                        let key = location.selection_key();
                        let generation = *refresh.peek();
                        let offset = acceptance_overview.peek().len() as i64;
                        spawn(async move {
                            let environment_id = match location.scope { Some(Scope::Environment(id)) => Some(id), _ => None };
                            let query = search();
                            match poam_api::list_acceptances(offset, environment_id, &query).await {
                                Ok(page) if *acceptance_scope.peek() == key && *acceptance_search.peek() == query && *refresh.peek() == generation => { acceptance_overview.write().extend(page.items); acceptance_next.set(page.has_more); acceptance_error.set(None); }
                                Err(err) if *acceptance_scope.peek() == key && *acceptance_search.peek() == query && *refresh.peek() == generation => acceptance_error.set(Some(err.to_string())),
                                _ => return,
                            }
                            acceptance_loading.set(false);
                        });
                    }, "Load more risk acceptances" } }
                    if location.kind == Tab::Everything && !mixed_selected().is_empty() {
                        div { class: "bulk-bar poams-bulk", role: "group", aria_label: "Selected risk acceptances",
                            span { "{mixed_selected().len()} source decisions selected" }
                            button { r#type: "button", class: "btn btn-ghost xs focus-ring", disabled: mixed_busy() || mixed_selected().len() > 100 || mixed_eligible_count == 0 || acceptance_loading(), onclick: move |_| {
                                let chosen = mixed_selected();
                                let targets: Vec<_> = acceptance_overview().into_iter().filter(|entry| chosen.contains(&acceptance_id(entry)) && renewable(entry, !viewer, is_admin)).collect();
                                if targets.is_empty() || chosen.len() > 100 { return; }
                                let key = location.selection_key();
                                mixed_busy.set(true); mixed_outcome.set(None);
                                spawn(async move {
                                    let mut succeeded = BTreeSet::new(); let mut failures = Vec::new();
                                    for entry in &targets {
                                        let result = poam_api::renew_acceptance(entry).await;
                                        if *acceptance_scope.peek() != key { mixed_busy.set(false); return; }
                                        match result { Ok(_) => { succeeded.insert(acceptance_id(entry)); }, Err(error) => failures.push(format!("{} {}: {error}", entry.source.key(), entry.source_id)) }
                                    }
                                    mixed_selected.set(chosen.difference(&succeeded).copied().collect());
                                    mixed_outcome.set(Some(format!("{} renewed; {} failed; {} ineligible. {}", succeeded.len(), failures.len(), chosen.len() - targets.len(), failures.join("; "))));
                                    if !succeeded.is_empty() { refresh.set(refresh().wrapping_add(1)); }
                                    mixed_busy.set(false);
                                });
                            }, "Renew eligible decisions 90 days" }
                            button { r#type: "button", class: "btn btn-ghost xs focus-ring", disabled: mixed_busy(), onclick: move |_| { mixed_selected.set(BTreeSet::new()); mixed_outcome.set(None); }, "Clear" }
                            if mixed_selected().len() > 100 { span { "Select at most 100 source decisions." } }
                        }
                    }
                    if let Some(result) = mixed_outcome() { p { class: "poams-notice", role: "status", "{result}" } }
                    if can_show_more { button { r#type: "button", class: "pv-more pv-more-row focus-ring", onclick: move |_| visible.set(visible() + 50), "Show more plans" } }
                     if let Some(offset) = next() { button { r#type: "button", class: "btn btn-ghost focus-ring poams-more", disabled: loading() || bulk_busy(), onclick: move |_| { loading.set(true); error.set(None); let request_scope = location.selection_key(); spawn(async move {
                         let result = poam_api::list_poam_register(&location.list_query(offset)).await;
                         if *loaded_scope.peek() != request_scope { return; }
                         match result {
                            Ok(page) => { rows.write().extend(page.items); next.set(page.next_offset); }
                            Err(err) => error.set(Some(err.to_string())),
                        }
                        loading.set(false);
                    }); }, "Load next page" } }
                    if loading() && available > 0 { span { role: "status", "Loading more plans..." } }
                }
                if location.kind == Tab::Acceptances { AcceptanceRegister { key: "acceptances-{location.selection_key()}", location, refresh, catalog: catalog(), overview: acceptance_overview, environments: env_names.clone(), group_by: grouping(), sort_by: sort(), status_filter: status(), risk_filter: risk(), mine: mine(), me, search: search(), progress_column: progress_column(), owner_column: owner_column() } }
                if selection_count > 0 { div { class: "bulk-bar poams-bulk", role: "group", aria_label: "Selected remediation plans",
                    span { "{selection_count} plans selected" }
                    if !viewer {
                        select { class: "cfgx-select focus-ring", aria_label: "Reassign selected owners", value: "", disabled: !can_assign || catalog().is_none(), onchange: move |event| {
                            if let Some(assignee) = catalog().as_ref().and_then(|data| assignee_from_catalog(&event.value(), data)) {
                                assign_bulk(BulkAction::Assign(assignee));
                            }
                        },
                            option { value: "", "Reassign owner..." }
                            if let Some(data) = catalog() {
                                option { value: "unassigned", "Unassigned" }
                                for person in data.people { option { value: "user:{person.user_id}", "{person.label}" } }
                                for group in data.groups { option { value: "group:{group.group_name}", "Group: {group.group_name}" } }
                            }
                        }
                        select { class: "cfgx-select focus-ring", aria_label: "Set selected plan status", value: "", disabled: !has_legal_statuses, onchange: move |event| {
                            let target = match event.value().as_str() { "open" => Some(PoamStatus::Open), "in_progress" => Some(PoamStatus::InProgress), "blocked" => Some(PoamStatus::Blocked), "awaiting_verification" => Some(PoamStatus::AwaitingVerification), _ => None };
                            if let Some(status) = target { status_bulk(BulkAction::Status(status)); }
                        },
                            option { value: "", "Set status..." }
                            for status in legal_statuses { option { value: "{status_key(status)}", "{status.label()}" } }
                        }
                        button { r#type: "button", class: "btn btn-ghost xs focus-ring", disabled: !can_extend, onclick: move |_| extend_bulk(BulkAction::Extend), "Extend due 30 days" }
                    }
                    button { r#type: "button", class: "btn btn-ghost xs focus-ring", disabled: bulk_busy(), onclick: move |_| { selected.set(BTreeSet::new()); anchor.set(None); bulk_outcome.set(None); }, "Clear" }
                    if bulk_busy() { span { role: "status", "Updating selected plans sequentially. Stay on this page until the batch finishes." } }
                    if selection_count > MAX_BULK_PLANS { span { class: "poam-muted", "Select at most 100 plans for a bulk update." } }
                    if !can_extend && !bulk_busy() { span { class: "poam-muted", "Extend requires a valid due date on every selected active plan." } }
                    if let Some(message) = catalog_error() { span { class: "poam-muted", "Owner catalog unavailable: {message}" } }
                } }
                if let Some(result) = bulk_outcome() { div { class: "poams-bulk-result", role: "status",
                    p { "{result.succeeded.len()} plan(s) confirmed; {result.failed.len()} not confirmed. Only confirmed IDs were cleared. A transport failure can occur after a server write; inspect the refreshed row before retrying." }
                    for (id, reason) in result.failed { p { class: "poam-overdue", "{id}: {reason}" } }
                } }
                if let Some(message) = refresh_error() { div { class: "poams-notice", role: "alert",
                    "The batch finished, but the register could not refresh: {message}. Displayed revisions may be stale. Further bulk updates are blocked."
                    button { r#type: "button", class: "btn btn-ghost xs focus-ring", disabled: loading() || bulk_busy(), onclick: move |_| { loading.set(true); spawn(async move {
                        match refresh_loaded_pages(location, rows().len()).await {
                            Ok((fresh, cursor)) => { rows.set(fresh); next.set(cursor); refresh_error.set(None); }
                            Err(err) => refresh_error.set(Some(err.to_string())),
                        }
                        loading.set(false);
                    }); }, "Retry refresh" }
                } }
            }
            if let Some(entry) = mixed_open().and_then(|id| acceptance_rows.iter().find(|entry| acceptance_id(entry) == id).cloned()) {
                AcceptanceTray { key: "mixed-{entry.source:?}:{entry.source_id}", entry, environments: env_names.clone(), operator: !viewer, admin: is_admin, catalog: catalog(), on_close: move |_| mixed_open.set(None), on_changed: move |_| { refresh.set(refresh().wrapping_add(1)); } }
            }
            PoamDetailHost { poam_id: detail_id, viewer, on_close: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { poam: None, ..location }.query() }); }, on_open_finding: move |finding: poam_api::FindingView| { nav.push(Route::SystemDetailView { id: finding.system_id.to_string(), tab: "compliance".into(), poam: String::new(), config_mode: String::new(), revision: String::new(), generation: String::new(), deploy_generation: String::new(), cve_target: String::new(), cve_mode: String::new() }); } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acceptance_identity_and_search_use_source_scope_without_inferred_membership() {
        let host: AcceptanceEntry = serde_json::from_value(serde_json::json!({
            "source": "cve_host", "human_id": "RA-0007", "source_id": Uuid::from_u128(7),
            "waiver_updated_at": null, "status": "accepted", "finding_id": null,
            "system_id": Uuid::from_u128(8), "system_hostname": "sledge", "environment_id": null,
            "policy_lineage_id": null, "policy_version_id": null,
            "canonical_cve_id": "CVE-2024-1234", "canonical_package_name": "openssl",
            "justification": "Reviewed", "review_date": "2026-10-01", "review_due_at": null,
            "expires_at": null, "accepted_by": null, "accepted_at": "2026-09-20T00:00:00Z",
            "retired_at": null, "retired_by": null, "retirement_reason": null,
            "replacement_poam_id": null, "recorded_at": "2026-09-20T00:00:00Z"
        }))
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 9, 29).unwrap();
        let location = RegisterLocation::parse("");
        assert_eq!(
            acceptance_row_title(&host, &[]),
            "CVE-2024-1234 — openssl on sledge"
        );
        assert_eq!(
            acceptance_group_keys(&host, "environment", &[], None, today),
            vec!["Host-only decisions"]
        );
        assert!(acceptance_matches(
            &host,
            location,
            today,
            "all",
            "all",
            false,
            None,
            "ra-0007",
            &[]
        ));
        assert!(acceptance_matches(
            &host,
            location,
            today,
            "all",
            "all",
            false,
            None,
            "sledge",
            &[]
        ));
        assert!(!acceptance_matches(
            &host,
            location,
            today,
            "all",
            "all",
            false,
            None,
            "Production",
            &[]
        ));
        assert_eq!(acceptance_group_focus("environment", &[host.clone()]), None);
        let mut next = host.clone();
        next.human_id = "RA-0008".into();
        next.source_id = Uuid::from_u128(1);
        assert!(acceptance_compare(&host, &next, "id").is_lt());
        assert_eq!(
            acceptance_id(&host),
            (AcceptanceSource::CveHost, Uuid::from_u128(7))
        );
        assert_eq!(acceptance_review_relative(&host, today), "in 2d");
        let mut policy = host.clone();
        policy.source = AcceptanceSource::PolicyWaiver;
        policy.canonical_cve_id = None;
        policy.canonical_package_name = None;
        policy.finding_id = Some(Uuid::from_u128(9));
        policy.policy_title = Some("Exact policy title".into());
        policy.requirement_external_id = Some("REQ-42".into());
        assert_eq!(
            acceptance_row_title(&policy, &[]),
            "Exact policy title on sledge"
        );
        assert!(acceptance_drawer_subject(&policy).contains("REQ-42"));
        for needle in ["exact policy", "req-42"] {
            assert!(acceptance_matches(
                &policy,
                location,
                today,
                "all",
                "all",
                false,
                None,
                needle,
                &[]
            ));
        }
        let mut environment = host.clone();
        environment.source = AcceptanceSource::CveEnvironment;
        environment.system_id = None;
        environment.system_hostname = None;
        environment.environment_id = Some(Uuid::from_u128(10));
        environment.environment_name = Some("Production".into());
        assert!(acceptance_matches(
            &environment,
            location,
            today,
            "all",
            "all",
            false,
            None,
            "production",
            &[]
        ));
        assert!(!acceptance_matches(
            &host,
            location,
            today,
            "all",
            "all",
            false,
            None,
            "production",
            &[]
        ));
    }

    #[test]
    fn bulk_status_matches_server_transition_matrix() {
        for from in [
            PoamStatus::Open,
            PoamStatus::InProgress,
            PoamStatus::Blocked,
        ] {
            for to in [
                PoamStatus::Open,
                PoamStatus::InProgress,
                PoamStatus::Blocked,
                PoamStatus::AwaitingVerification,
            ] {
                assert_eq!(legal_transition(from, to), from != to);
            }
        }
        assert!(legal_transition(
            PoamStatus::AwaitingVerification,
            PoamStatus::Blocked
        ));
        assert!(legal_transition(
            PoamStatus::AwaitingVerification,
            PoamStatus::InProgress
        ));
        assert!(!legal_transition(
            PoamStatus::AwaitingVerification,
            PoamStatus::Open
        ));
        for from in [
            PoamStatus::Open,
            PoamStatus::InProgress,
            PoamStatus::Blocked,
            PoamStatus::AwaitingVerification,
            PoamStatus::Completed,
        ] {
            assert!(!legal_transition(from, PoamStatus::Completed));
            assert!(!legal_transition(PoamStatus::Completed, from));
        }
    }

    #[test]
    fn extend_uses_calendar_days_only_for_existing_dates() {
        assert_eq!(
            extended_date(NaiveDate::from_ymd_opt(2024, 2, 29)),
            NaiveDate::from_ymd_opt(2024, 3, 30)
        );
        assert_eq!(extended_date(None), None);
        assert_eq!(extended_date(NaiveDate::MAX.into()), None);
    }

    #[test]
    fn batch_limits_and_exact_selected_ids_prevent_partial_action() {
        let mut rows: Vec<_> = (1..=101).map(register_row).collect();
        let mut ids: BTreeSet<_> = rows.iter().take(100).map(|row| row.summary.id).collect();
        let action = BulkAction::Status(PoamStatus::InProgress);
        let targets = batch_targets(&rows, &ids, &action).unwrap();
        assert_eq!(targets.len(), MAX_BULK_PLANS);
        assert_eq!(targets[0].0, rows[0].summary.id);
        ids.insert(rows[100].summary.id);
        assert!(batch_targets(&rows, &ids, &action).is_none());
        ids.remove(&rows[100].summary.id);
        rows[0].summary.status = PoamStatus::Completed;
        assert!(batch_targets(&rows, &ids, &action).is_none());
        ids.remove(&rows[0].summary.id);
        assert!(batch_targets(&rows, &ids, &BulkAction::Extend).is_none());
        assert!(batch_targets(&rows, &BTreeSet::from([Uuid::from_u128(999)]), &action).is_none());
    }

    #[test]
    fn partial_results_clear_only_successful_ids() {
        let one = Uuid::from_u128(1);
        let two = Uuid::from_u128(2);
        let three = Uuid::from_u128(3);
        let selected = BTreeSet::from([one, two, three]);
        let mut outcome = BulkOutcome::default();
        record_result(&mut outcome, one, Ok(()));
        record_result(&mut outcome, two, Err("stale revision".into()));
        record_result(&mut outcome, three, Ok(()));
        assert_eq!(
            remaining_selection(&selected, &outcome),
            BTreeSet::from([two])
        );
        assert_eq!(outcome.failed, vec![(two, "stale revision".into())]);
    }

    #[test]
    fn assignee_choices_require_catalog_identity() {
        let user = Uuid::from_u128(22);
        let catalog = PoamAssigneeCatalog {
            people: vec![poam_api::PoamAssigneePerson {
                user_id: user,
                label: "Operator".into(),
            }],
            groups: vec![poam_api::PoamAssigneeGroup {
                group_name: "assessors".into(),
            }],
        };
        assert_eq!(
            assignee_from_catalog(&format!("user:{user}"), &catalog),
            Some(PoamAssigneeRequest::User { user_id: user })
        );
        assert_eq!(
            assignee_from_catalog("group:assessors", &catalog),
            Some(PoamAssigneeRequest::OidcGroup {
                group_name: "assessors".into()
            })
        );
        assert!(
            assignee_from_catalog("user:00000000-0000-0000-0000-000000000099", &catalog).is_none()
        );
    }

    #[test]
    fn route_round_trip_preserves_typed_scope_queue_kind_and_detail() {
        let state = RegisterLocation {
            poam: Some(Uuid::from_u128(10)),
            kind: Tab::Plans,
            queue: Some(Queue::Blocked),
            scope: Some(Scope::System(Uuid::from_u128(20))),
            dimension: Dimension::Environment,
        };
        assert_eq!(RegisterLocation::parse(&state.query()), state);
        let query = state.list_query(100);
        assert_eq!(query.system_id, Some(Uuid::from_u128(20)));
        assert_eq!(query.offset, Some(100));
        assert_eq!(
            RegisterLocation::parse("poam=bad&system=bad&queue=unknown"),
            RegisterLocation {
                poam: None,
                kind: Tab::Everything,
                queue: None,
                scope: None,
                dimension: Dimension::Environment,
            }
        );
        let env = RegisterLocation {
            scope: Some(Scope::Environment(Uuid::from_u128(30))),
            ..state
        };
        assert_eq!(env.list_query(0).system_id, None);
        assert!(env.partial_scope());
        let owner = RegisterLocation {
            scope: Some(Scope::Owner(Uuid::from_u128(40))),
            dimension: Dimension::Owner,
            ..state
        };
        assert_eq!(RegisterLocation::parse(&owner.query()), owner);
        assert!(owner.partial_scope());
        let bundle = RegisterLocation {
            scope: Some(Scope::Bundle(Uuid::from_u128(50))),
            dimension: Dimension::Bundle,
            ..state
        };
        assert_eq!(bundle.list_query(0).bundle_id, Some(Uuid::from_u128(50)));
        assert!(bundle.partial_scope());
    }

    fn register_row(id: u128) -> PoamRegisterSummary {
        let json = serde_json::json!({
            "id":Uuid::from_u128(id),"human_id":"POAM-1","title":"Fix","plan":"Patch","owner":"", "assignee":{"kind":"unassigned"},"target_date":null,"risk":"high","status":"open","revision":1,"overdue":false,"finding_count":1,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","closed_at":null,"closure_attempt_id":null,
            "environment_ids":[Uuid::from_u128(2)],"system_ids":[Uuid::from_u128(3)],"bundle_ids":[Uuid::from_u128(4)],"bundle_version_ids":[],"assignment_version_ids":[],"first_requirement":null,"first_cve":null,"milestone_count":0,"completed_milestone_count":0,"last_activity_at":null
        });
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn subtitles_count_only_visible_members_in_the_displayed_environment() {
        let a = Uuid::from_u128(2);
        let b = Uuid::from_u128(5);
        let environments: Vec<EnvironmentSummary> = serde_json::from_value(serde_json::json!([
            {"id":a,"name":"edge","description":null,"color_hex":"#123456","is_active":true,"system_count":1},
            {"id":b,"name":"core","description":null,"color_hex":"#654321","is_active":true,"system_count":2}
        ])).unwrap();
        let mut row = register_row(1);
        row.environment_ids = vec![a, b];
        row.first_cve = Some("CVE-2026-27442".into());
        row.system_ids = vec![Uuid::from_u128(3), Uuid::from_u128(6), Uuid::from_u128(7)];
        row.systems = vec![
            poam_api::RegisterSystemScope {
                system_id: Uuid::from_u128(3),
                hostname: "edge-node".into(),
                environment_id: Some(a),
            },
            poam_api::RegisterSystemScope {
                system_id: Uuid::from_u128(6),
                hostname: "core-one".into(),
                environment_id: Some(b),
            },
            poam_api::RegisterSystemScope {
                system_id: Uuid::from_u128(7),
                hostname: "core-two".into(),
                environment_id: Some(b),
            },
        ];
        assert_eq!(
            plan_subtitle(&row, Some(a), true, None, &environments),
            "edge-node · edge"
        );
        assert_eq!(
            plan_subtitle(&row, Some(b), true, None, &environments),
            "2 hosts · core"
        );
        assert_eq!(row.summary.id, Uuid::from_u128(1));
        row.environment_ids = vec![b];
        assert_eq!(
            plan_subtitle(&row, Some(b), true, None, &environments),
            "2 hosts · core · CVE-2026-27442"
        );
        assert_eq!(
            group_distribution(&[&row], &[]),
            vec![("Open", "#60a5fa", 1)]
        );
    }

    #[test]
    fn host_focused_subtitle_uses_exact_system_edge_in_single_and_multi_env_plans() {
        let a = Uuid::from_u128(2);
        let b = Uuid::from_u128(5);
        let host = Uuid::from_u128(3);
        let other = Uuid::from_u128(6);
        let environments: Vec<EnvironmentSummary> = serde_json::from_value(serde_json::json!([
            {"id":a,"name":"edge","description":null,"color_hex":"#123456","is_active":true,"system_count":1},
            {"id":b,"name":"core","description":null,"color_hex":"#654321","is_active":true,"system_count":1}
        ])).unwrap();
        let mut row = register_row(1);
        row.systems = vec![poam_api::RegisterSystemScope {
            system_id: host,
            hostname: "edge-node".into(),
            environment_id: Some(a),
        }];
        assert_eq!(
            plan_subtitle(&row, Some(a), true, Some(host), &environments),
            "edge-node · edge"
        );
        row.environment_ids.push(b);
        row.system_ids.push(other);
        row.systems.push(poam_api::RegisterSystemScope {
            system_id: other,
            hostname: "core-node".into(),
            environment_id: Some(b),
        });
        // Grouping by the other environment must not hide the selected host
        // or attribute the other environment to it.
        assert_eq!(
            plan_subtitle(&row, Some(b), true, Some(host), &environments),
            "edge-node · edge"
        );
        assert_eq!(
            plan_subtitle(&row, None, false, Some(other), &environments),
            "core-node · core"
        );
        row.systems.retain(|system| system.system_id != host);
        assert_eq!(
            plan_subtitle(&row, Some(b), true, Some(host), &environments),
            "1 host"
        );
    }

    #[test]
    fn grouped_selection_follows_expanded_visible_rows_and_deduplicates() {
        let rows: Vec<_> = (1..=104).map(register_row).collect();
        let mut groups = BTreeMap::new();
        groups.insert("A".to_string(), vec![&rows[0], &rows[1], &rows[2]]);
        groups.insert("B".to_string(), vec![&rows[2], &rows[3], &rows[4]]);
        let visible = displayed_order(&groups, &BTreeSet::new(), 2);
        assert_eq!(
            visible,
            vec![
                rows[0].summary.id,
                rows[1].summary.id,
                rows[2].summary.id,
                rows[3].summary.id
            ]
        );
        let collapsed = BTreeSet::from(["A".to_string()]);
        let order = displayed_order(&groups, &collapsed, 2);
        assert_eq!(order, vec![rows[2].summary.id, rows[3].summary.id]);
        let mut selected = BTreeSet::new();
        let mut anchor = Some(order[0]);
        select_row(&mut selected, &mut anchor, order[1], &order, true, false);
        assert_eq!(selected, order.into_iter().collect());
    }

    #[test]
    fn scope_pills_include_only_authorized_page_memberships() {
        let mut rows = vec![register_row(1), register_row(2)];
        let a = Uuid::from_u128(2);
        let b = Uuid::from_u128(9);
        let host = Uuid::from_u128(3);
        let other = Uuid::from_u128(7);
        let env = Scope::Environment(a);
        assert_eq!(
            scope_pills(&rows, "environment", None, &[], &[], None),
            vec![(env, "Environment unavailable".into(), 2)]
        );
        // Plan-wide system_ids do not prove an environment membership.
        assert!(scope_pills(&rows, "environment", Some(env), &[], &[], None).is_empty());
        rows[0].systems = vec![poam_api::RegisterSystemScope {
            system_id: host,
            hostname: "ata".into(),
            environment_id: Some(a),
        }];
        rows[1].systems = rows[0].systems.clone();
        assert_eq!(
            scope_pills(&rows, "environment", Some(env), &[], &[], None),
            vec![(Scope::System(host), "ata".into(), 2)]
        );
        assert!(scope_pills(&rows, "owner", None, &[], &[], None).is_empty());
        rows[0].environment_ids.push(b);
        rows[0].system_ids.push(other);
        rows[0].systems.push(poam_api::RegisterSystemScope {
            system_id: other,
            hostname: "lan".into(),
            environment_id: Some(b),
        });
        assert_eq!(
            scope_pills(&rows, "environment", Some(env), &[], &[], None),
            vec![(Scope::System(host), "ata".into(), 2)]
        );
        assert_eq!(
            scope_pills(
                &rows,
                "environment",
                Some(Scope::Environment(b)),
                &[],
                &[],
                None
            ),
            vec![(Scope::System(other), "lan".into(), 1)]
        );
        rows[1].environment_ids.push(b);
        rows[1].system_ids.push(other);
        // A plan-wide environment and host ID without the paired system edge
        // cannot create a second LAN membership.
        assert_eq!(
            scope_pills(
                &rows,
                "environment",
                Some(Scope::Environment(b)),
                &[],
                &[],
                None
            ),
            vec![(Scope::System(other), "lan".into(), 1)]
        );
        rows[0].systems[1].environment_id = None;
        assert!(
            scope_pills(
                &rows,
                "environment",
                Some(Scope::Environment(b)),
                &[],
                &[],
                None
            )
            .is_empty()
        );
    }

    #[test]
    fn selection_uses_exact_loaded_order_and_adds_reverse_ranges() {
        let order: Vec<_> = (1..=105).map(Uuid::from_u128).collect();
        let mut ids = BTreeSet::new();
        let mut anchor = None;
        assert!(select_row(
            &mut ids,
            &mut anchor,
            order[103],
            &order,
            false,
            true
        ));
        assert!(select_row(
            &mut ids,
            &mut anchor,
            order[0],
            &order,
            true,
            false
        ));
        assert_eq!(ids.len(), 104);
        assert!(select_row(
            &mut ids,
            &mut anchor,
            order[104],
            &order,
            false,
            true
        ));
        assert_eq!(ids.len(), 105);
        assert!(select_row(
            &mut ids,
            &mut anchor,
            order[104],
            &order,
            false,
            true
        ));
        assert_eq!(ids.len(), 104);
    }

    #[test]
    fn missing_anchor_selects_only_clicked_id() {
        let mut ids = BTreeSet::new();
        let mut anchor = Some(Uuid::from_u128(9));
        let order = [Uuid::from_u128(1), Uuid::from_u128(2)];
        select_row(&mut ids, &mut anchor, order[1], &order, true, false);
        assert_eq!(ids, BTreeSet::from([order[1]]));
        assert_eq!(anchor, Some(order[1]));
    }

    #[test]
    fn scope_uses_uuid_membership_not_display_names() {
        let row = register_row(1);
        assert!(in_scope(&row, Some(Scope::Environment(Uuid::from_u128(2)))));
        assert!(in_scope(&row, Some(Scope::System(Uuid::from_u128(3)))));
        assert!(in_scope(&row, Some(Scope::Bundle(Uuid::from_u128(4)))));
        assert!(!in_scope(
            &row,
            Some(Scope::Environment(Uuid::from_u128(5)))
        ));
        let today = NaiveDate::from_ymd_opt(2026, 9, 27).unwrap();
        assert!(Queue::Unassigned.includes(&row, today));
        assert!(!Queue::Soon.includes(&row, today));
        assert!(!Queue::Quiet.includes(&row, today));
    }
}
