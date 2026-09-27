//! Authenticated, page-backed POA&M register.
//!
//! All counts and scope memberships here describe loaded pages only. Risk
//! acceptance decisions are not POA&Ms and have no register reader yet.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Days, NaiveDate, Utc};
use dioxus::prelude::*;
use uuid::Uuid;

use crate::components::poam::PoamDetailHost;
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::views::poam_api::{
    self, AcceptanceEntry, AcceptanceSource, PoamApiError, PoamAssigneeCatalog,
    PoamAssigneeRequest, PoamAssigneeView, PoamListQuery, PoamRegisterSummary, PoamRisk,
    PoamStatus, TransitionPoamRequest, UpdatePoamRequest,
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

fn scope_pills(
    rows: &[PoamRegisterSummary],
    dimension: &str,
    current: Option<Scope>,
) -> Vec<(Scope, String, usize)> {
    let mut keys: BTreeMap<Scope, (String, BTreeSet<Uuid>)> = BTreeMap::new();
    for row in rows {
        let candidates: Vec<(Scope, String)> = match (dimension, current) {
            ("environment", Some(Scope::Environment(env))) => {
                // A flat projection cannot associate individual hosts with one
                // of several environments; do not invent that relationship.
                if row.environment_ids.as_slice() == [env] {
                    row.system_ids
                        .iter()
                        .map(|id| (Scope::System(*id), format!("Host {id}")))
                        .collect()
                } else {
                    Vec::new()
                }
            }
            ("environment", None) => row
                .environment_ids
                .iter()
                .map(|id| (Scope::Environment(*id), format!("Environment {id}")))
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
            entry.1.insert(row.summary.id);
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
        Some(Scope::Environment(id)) => {
            item.environment_id == Some(id)
                || item.system_id.is_some() && item.environment_id.is_none()
        }
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

#[component]
fn AcceptanceRegister(location: RegisterLocation) -> Element {
    let mut rows = use_signal(Vec::<AcceptanceEntry>::new);
    let mut total = use_signal(|| 0_i64);
    let mut has_more = use_signal(|| false);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut loaded_scope = use_signal(String::new);
    let environment_id = match location.scope {
        Some(Scope::Environment(id)) => Some(id),
        _ => None,
    };
    let scope_key = location.selection_key();
    use_effect(use_reactive(&scope_key, move |key| {
        loaded_scope.set(key.clone());
        rows.set(Vec::new());
        total.set(0);
        has_more.set(false);
        error.set(None);
        loading.set(true);
        spawn(async move {
            let result = poam_api::list_acceptances(0, environment_id).await;
            if *loaded_scope.peek() != key {
                return;
            }
            match result {
                Ok(page) => {
                    total.set(page.total);
                    has_more.set(page.has_more);
                    rows.set(page.items);
                }
                Err(err) => error.set(Some(err.to_string())),
            }
            loading.set(false);
        });
    }));
    let loaded = rows();
    let shown: Vec<_> = loaded
        .iter()
        .filter(|item| acceptance_in_scope(item, location.scope))
        .cloned()
        .collect();
    let partial_scope = matches!(
        location.scope,
        Some(Scope::System(_) | Scope::Owner(_) | Scope::Bundle(_))
    );
    rsx! {
        section { class: "card poams-main", aria_label: "Risk acceptances",
            div { class: "poams-scope",
                strong { "Risk acceptances" }
                span { "{shown.len()} shown / {loaded.len()} loaded / {total()} matching server scope"
                    if has_more() { " - more pages available" }
                    if partial_scope { " - this scope is partial across pages" }
                }
            }
            p { class: "poam-muted", style: "padding: 10px 18px;",
                "These decisions belong to their policy-waiver or CVE service. Renewal and conversion are not yet available here; opening or filtering cannot change a decision."
            }
            if loading() && loaded.is_empty() { p { class: "poams-notice", role: "status", "Loading accepted decisions..." } }
            if let Some(message) = error() { p { class: "poams-notice", role: "alert", "Could not load risk acceptances: {message}" } }
            if !loading() && error().is_none() && shown.is_empty() {
                p { class: "poams-notice", "No accepted decisions in the loaded scope." }
            }
            if !shown.is_empty() {
                div { class: "poams-table-wrap",
                    table { class: "sys-table compact sys-table-dense poams-table",
                        thead { tr { th { "SOURCE" } th { "SUBJECT" } th { "DECISION" } th { "REVIEW / EXPIRY" } } }
                        tbody { for item in shown {
                            tr { key: "{item.source:?}:{item.source_id}",
                                td { class: "mono", "{item.source_id}" }
                                td {
                                    strong { {match item.source { AcceptanceSource::PolicyWaiver => "Policy waiver", AcceptanceSource::CveHost => "Host CVE", AcceptanceSource::CveEnvironment => "Environment CVE" }} }
                                    small { class: "mono", "{acceptance_subject(&item)}" }
                                    small { if let Some(host) = item.system_id { "Host {host}" } else if let Some(env) = item.environment_id { "Environment {env}" } }
                                }
                                td { "{item.justification}" small { if item.retired_at.is_some() { "Historical decision (retired)" } else { "Accepted decision" } } }
                                td { if let Some(date) = item.review_date { "Review {date}" }
                                    else if let Some(expiry) = item.expires_at { "Authorization expires {expiry}" }
                                    else { "No review date recorded" }
                                }
                            }
                        } }
                    }
                }
            }
            if has_more() { button { r#type: "button", class: "btn btn-ghost focus-ring poams-more", disabled: loading(), onclick: move |_| {
                loading.set(true);
                let offset = rows().len() as i64;
                let key = location.selection_key();
                spawn(async move {
                    let result = poam_api::list_acceptances(offset, environment_id).await;
                    if *loaded_scope.peek() != key { return; }
                    match result {
                        Ok(page) => { rows.write().extend(page.items); total.set(page.total); has_more.set(page.has_more); error.set(None); }
                        Err(err) => error.set(Some(err.to_string())),
                    }
                    loading.set(false);
                });
            }, "Load more accepted decisions" } }
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
    let mut grouping = use_signal(|| "none".to_string());
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
    let nav = use_navigator();
    let detail_id = location.poam;

    // CONCURRENCY: A URL scope change can finish after a previous list request.
    // Discard that response so it cannot replace the new scope's rows.
    let scope_key = location.selection_key();
    use_effect(use_reactive(&scope_key, move |key| {
        loaded_scope.set(key.clone());
        rows.set(Vec::new());
        next.set(None);
        selected.write().clear();
        anchor.set(None);
        error.set(None);
        loading.set(true);
        spawn(async move {
            let result = poam_api::list_poam_register(&location.list_query(0)).await;
            if *loaded_scope.peek() != key {
                return;
            }
            match result {
                Ok(page) => {
                    rows.set(page.items);
                    next.set(page.next_offset);
                }
                Err(err) => error.set(Some(err.to_string())),
            }
            loading.set(false);
        });
    }));
    use_future(move || async move {
        if !viewer {
            match poam_api::fetch_assignee_catalog().await {
                Ok(data) => catalog.set(Some(data)),
                Err(error) => catalog_error.set(Some(error.to_string())),
            }
        }
    });

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
    let page_label = if loading() {
        "loading page; total unknown"
    } else if error().is_some() {
        "page load failed; total unknown"
    } else if has_more {
        "more pages available"
    } else {
        "all pages loaded"
    };
    let scope_label = location.scope.map(|s| match s {
        Scope::Environment(id) => format!("Environment {id}"),
        Scope::System(id) => format!("Host {id}"),
        Scope::Bundle(id) => format!("Bundle {id}"),
        Scope::Owner(id) => format!("Owner {id}"),
    });
    let available = loaded.len();
    let open = loaded
        .iter()
        .filter(|r| r.summary.status.is_active())
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
    let mut groups: BTreeMap<String, Vec<&PoamRegisterSummary>> = BTreeMap::new();
    for row in ordered {
        let keys: Vec<String> = match grouping().as_str() {
            "environment" => row
                .environment_ids
                .iter()
                .map(|id| format!("Environment {id}"))
                .collect(),
            "system" => row
                .system_ids
                .iter()
                .map(|id| format!("Host {id}"))
                .collect(),
            "bundle" => row
                .bundle_ids
                .iter()
                .map(|id| format!("Bundle {id}"))
                .collect(),
            "owner" => vec![owner(row)],
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
    let can_show_more = groups.values().any(|entries| entries.len() > visible());
    let pills = scope_pills(&loaded, location.dimension.key(), location.scope);
    let scope_partial = location.partial_scope();
    let scope_notice = if scope_partial {
        " - scope is partial across pages"
    } else {
        ""
    };
    let host_id = match location.scope {
        Some(Scope::System(id)) if loaded.iter().any(|row| row.system_ids.contains(&id)) => {
            Some(id)
        }
        _ => None,
    };
    let parent_env = host_id.and_then(|host| {
        let mut envs = loaded
            .iter()
            .filter(|row| row.system_ids.contains(&host))
            .flat_map(|row| row.environment_ids.iter().copied())
            .collect::<BTreeSet<_>>();
        if envs.len() == 1 {
            envs.pop_first()
        } else {
            None
        }
    });
    let row_view = |row: &PoamRegisterSummary| {
        let p = &row.summary;
        let id = p.id;
        let due = p
            .target_date
            .map(|date| date.to_string())
            .unwrap_or_else(|| "No date".into());
        let status_label = p.status.label();
        let risk_label = p.risk.category_label();
        let owner_label = owner(row);
        let owner_id = match &p.assignee {
            Some(PoamAssigneeView::User { user_id, .. }) => Some(*user_id),
            _ => None,
        };
        let context = row
            .first_requirement
            .as_deref()
            .or(row.first_cve.as_deref())
            .unwrap_or("No linked requirement or CVE");
        let checked = selected().contains(&id);
        let order = order.clone();
        let environments = row.environment_ids.clone();
        let systems = row.system_ids.clone();
        let bundles = row.bundle_ids.clone();
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
                td { strong { "{p.title}" } small { "{context}" }
                    div { class: "poams-row-scope",
                        for env in environments { button { r#type: "button", class: "pv-link focus-ring", onclick: move |e| { e.stop_propagation(); nav.push(Route::PoamsView { query: RegisterLocation { scope: Some(Scope::Environment(env)), poam: None, ..location }.query() }); }, "Environment {env}" } }
                        for host in systems { button { r#type: "button", class: "pv-link focus-ring", onclick: move |e| { e.stop_propagation(); nav.push(Route::PoamsView { query: RegisterLocation { scope: Some(Scope::System(host)), poam: None, ..location }.query() }); }, "Host {host}" } }
                        for bundle in bundles { button { r#type: "button", class: "pv-link focus-ring", onclick: move |e| { e.stop_propagation(); nav.push(Route::PoamsView { query: RegisterLocation { dimension: Dimension::Bundle, scope: Some(Scope::Bundle(bundle)), poam: None, ..location }.query() }); }, "Bundle {bundle}" } }
                    }
                }
                td { "{risk_label}" } td { "{status_label}" }
                if progress_column() { td { "{row.completed_milestone_count}/{row.milestone_count} milestones" } }
                if owner_column() { td { if let Some(user_id) = owner_id { button { r#type: "button", class: "pv-link focus-ring", onclick: move |e| { e.stop_propagation(); nav.push(Route::PoamsView { query: RegisterLocation { dimension: Dimension::Owner, scope: Some(Scope::Owner(user_id)), poam: None, ..location }.query() }); }, "{owner_label}" } } else { "{owner_label}" } } }
                td { "{due}" }
            }
        }
    };

    rsx! {
        div { class: "poams-register",
            div { class: "page-head", div { h1 { class: "page-title", "POA&M" }
                p { class: "page-subtitle", "{open} open plans in {available} loaded - accepted decisions read separately - {page_label}" }
            } }
            div { class: "rr-kinds", role: "tablist", aria_label: "Record type",
                for (kind, label) in [(Tab::Everything, "Everything"), (Tab::Plans, "Remediation plans"), (Tab::Acceptances, "Risk acceptances")] {
                    button { r#type: "button", role: "tab", class: if location.kind == kind { "rr-kind active focus-ring" } else { "rr-kind focus-ring" }, aria_selected: if location.kind == kind { "true" } else { "false" }, onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { kind, queue: if kind == Tab::Acceptances { None } else { location.queue }, poam: None, ..location }.query() }); }, "{label}" }
                }
            }
            if location.kind != Tab::Plans { AcceptanceRegister { key: "acceptances-{location.selection_key()}", location } }
            if location.kind != Tab::Acceptances {
                div { class: "pv-queues", role: "group", aria_label: "Work queues",
                    for q in [Queue::Overdue, Queue::Soon, Queue::Awaiting, Queue::Blocked, Queue::Quiet, Queue::Unassigned] {
                        button { r#type: "button", class: if location.queue == Some(q) { "pv-q active focus-ring" } else { "pv-q focus-ring" }, aria_pressed: if location.queue == Some(q) { "true" } else { "false" }, onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { queue: if location.queue == Some(q) { None } else { Some(q) }, poam: None, ..location }.query() }); },
                            strong { "{loaded.iter().filter(|r| in_scope(r, location.scope) && q.includes(r, today)).count()}" } span { "{q.label()}" }
                        }
                    }
                }
                p { class: "poam-muted", "Work queue counts are for loaded pages only. They may grow as more pages load." }
                div { class: "card poams-main",
                    div { class: "poams-scope",
                        div { class: "seg xs", role: "tablist", aria_label: "Browse by",
                            for (key, label) in [("environment", "Environment"), ("bundle", "Bundle"), ("owner", "Owner / Approver")] {
                                button { r#type: "button", role: "tab", aria_selected: if location.dimension.key() == key { "true" } else { "false" }, class: if location.dimension.key() == key { "active" } else { "" }, onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { dimension: Dimension::parse(key), scope: None, poam: None, ..location }.query() }); }, "{label}" }
                            }
                        }
                        nav { aria_label: "Scope", class: "poams-crumb",
                            button { r#type: "button", class: "pv-link focus-ring", onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { scope: None, poam: None, ..location }.query() }); }, "All" }
                            if let Some(env) = parent_env { button { r#type: "button", class: "pv-link focus-ring", onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { scope: Some(Scope::Environment(env)), poam: None, ..location }.query() }); }, " / Environment {env}" } }
                            if let Some(label) = scope_label { strong { " / {label}" } }
                        }
                        span { "{shown_count} shown / {matching} matching loaded / {available} loaded - {page_label}{scope_notice}" }
                        if let Some(host) = host_id { button { r#type: "button", class: "btn btn-ghost xs focus-ring", onclick: move |_| { nav.push(Route::SystemDetailView { id: host.to_string(), tab: "compliance".into(), poam: String::new(), config_mode: String::new(), revision: String::new(), generation: String::new(), deploy_generation: String::new(), cve_target: String::new(), cve_mode: String::new() }); }, "Open host" } }
                    }
                    if !pills.is_empty() {
                        div { class: "poams-pills", aria_label: "Narrow to loaded scopes",
                            for (key, label, count) in pills.iter().take(7).cloned() {
                                button { r#type: "button", class: "rr-pill focus-ring", onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { scope: Some(key), poam: None, ..location }.query() }); }, span { "{label}" } span { "{count}" } }
                            }
                            if pills.len() > 7 { div { class: "poams-overflow",
                                button { r#type: "button", class: "rr-pill focus-ring", aria_expanded: if overflow_open() { "true" } else { "false" }, onclick: move |_| overflow_open.set(!overflow_open()), "+{pills.len() - 7} more" }
                                if overflow_open() { div { class: "card poams-picker", role: "dialog", aria_label: "More loaded scopes",
                                    input { class: "input focus-ring", aria_label: "Find loaded scope", placeholder: "Find UUID or owner", value: "{overflow_search()}", oninput: move |e| overflow_search.set(e.value()) }
                                    div { class: "poams-picker-list", for (key, label, count) in pills.iter().skip(7).filter(|(_, label, _)| label.to_lowercase().contains(&overflow_search().to_lowercase())).cloned() {
                                        button { r#type: "button", class: "rr-pill focus-ring", onclick: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { scope: Some(key), poam: None, ..location }.query() }); }, "{label} ({count} loaded)" }
                                    } }
                                    button { r#type: "button", class: "btn btn-ghost xs focus-ring", onclick: move |_| overflow_open.set(false), "Close" }
                                } }
                            } }
                        }
                    }
                    div { class: "poams-toolbar",
                        input { class: "input focus-ring", aria_label: "Search loaded plans", placeholder: "Search loaded ID, title, requirement, CVE", value: "{search()}", oninput: move |e| { search.set(e.value()); visible.set(50); selected.set(BTreeSet::new()); anchor.set(None); } }
                        select { aria_label: "Status", class: "cfgx-select focus-ring", value: "{status()}", onchange: move |e| { status.set(e.value()); selected.set(BTreeSet::new()); anchor.set(None); }, option { value: "active", "Active" } option { value: "closed", "Closed" } option { value: "all", "All" } }
                        select { aria_label: "Risk", class: "cfgx-select focus-ring", value: "{risk()}", onchange: move |e| { risk.set(e.value()); selected.set(BTreeSet::new()); anchor.set(None); }, option { value: "all", "Any risk" } option { value: "high", "CAT I" } option { value: "medium", "CAT II" } option { value: "low", "CAT III" } }
                        button { r#type: "button", class: "btn btn-ghost xs focus-ring", disabled: me.is_none(), aria_pressed: if mine() { "true" } else { "false" }, onclick: move |_| { mine.set(!mine()); selected.set(BTreeSet::new()); anchor.set(None); }, "Mine" }
                        select { aria_label: "Group loaded plans", class: "cfgx-select focus-ring", value: "{grouping()}", onchange: move |e| { grouping.set(e.value()); selected.set(BTreeSet::new()); anchor.set(None); }, option { value: "none", "No grouping" } option { value: "environment", "Environment" } option { value: "system", "Host" } option { value: "bundle", "Bundle" } option { value: "owner", "Owner" } option { value: "due", "Due" } }
                        select { aria_label: "Sort loaded plans", class: "cfgx-select focus-ring", value: "{sort()}", onchange: move |e| { sort.set(e.value()); selected.set(BTreeSet::new()); anchor.set(None); }, option { value: "urgency", "Urgency" } option { value: "due", "Due" } option { value: "risk", "Risk" } option { value: "activity", "Last activity" } option { value: "id", "ID" } }
                        label { input { r#type: "checkbox", checked: progress_column(), onchange: move |e| progress_column.set(e.checked()) } "Progress" }
                        label { input { r#type: "checkbox", checked: owner_column(), onchange: move |e| owner_column.set(e.checked()) } "Owner" }
                    }
                    p { class: "poam-muted", "Search, sort, grouping, scope and queue filters apply to loaded pages. Ctrl/Cmd toggles a row; Shift adds the range in the displayed order." }
                    if loading() && available == 0 { div { class: "poams-notice", role: "status", "Loading plans..." } }
                    if let Some(err) = error() { div { class: "poams-notice", role: "alert", "Could not load plans: {err}" } }
                    if !loading() && matching == 0 && error().is_none() { div { class: "poams-notice", "No matching plans in loaded pages." } }
                    for (name, entries) in groups {
                        section { class: "poams-group", key: "{name}", if grouping() != "none" { button { r#type: "button", class: "poams-group-toggle focus-ring", aria_expanded: if collapsed().contains(&name) { "false" } else { "true" }, onclick: { let name = name.clone(); move |_| { let mut set = collapsed(); if !set.insert(name.clone()) { set.remove(&name); } collapsed.set(set); selected.set(BTreeSet::new()); anchor.set(None); } }, "{name} - {entries.len()} loaded" } }
                            if !collapsed().contains(&name) {
                            div { class: "poams-table-wrap", table { class: "sys-table compact sys-table-dense poams-table", thead { tr { th { "ID" } th { "Title" } th { "Risk" } th { "Status" } if progress_column() { th { "Progress" } } if owner_column() { th { "Owner" } } th { "Due" } } }
                                tbody { for row in entries.into_iter().take(visible()) { {row_view(row)} } }
                            } }
                            }
                        }
                    }
                    if can_show_more { button { r#type: "button", class: "btn btn-ghost focus-ring poams-more", onclick: move |_| visible.set(visible() + 50), "Show more loaded plans in each group" } }
                     if let Some(offset) = next() { button { r#type: "button", class: "btn btn-ghost focus-ring poams-more", disabled: loading() || bulk_busy(), onclick: move |_| { loading.set(true); error.set(None); let request_scope = location.selection_key(); spawn(async move {
                         let result = poam_api::list_poam_register(&location.list_query(offset)).await;
                         if *loaded_scope.peek() != request_scope { return; }
                         match result {
                            Ok(page) => { rows.write().extend(page.items); next.set(page.next_offset); }
                            Err(err) => error.set(Some(err.to_string())),
                        }
                        loading.set(false);
                    }); }, "Load next page" } }
                    if loading() && available > 0 { span { role: "status", "Loading next page; showing {available} loaded plans." } }
                }
                if selection_count > 0 { div { class: "bulk-bar poams-bulk", role: "group", aria_label: "Selected remediation plans",
                    span { "{selection_count} loaded plans selected" }
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
            PoamDetailHost { poam_id: detail_id, viewer, on_close: move |_| { nav.push(Route::PoamsView { query: RegisterLocation { poam: None, ..location }.query() }); }, on_open_finding: move |finding: poam_api::FindingView| { nav.push(Route::SystemDetailView { id: finding.system_id.to_string(), tab: "compliance".into(), poam: String::new(), config_mode: String::new(), revision: String::new(), generation: String::new(), deploy_generation: String::new(), cve_target: String::new(), cve_mode: String::new() }); } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let rows = vec![register_row(1), register_row(2)];
        let env = Scope::Environment(Uuid::from_u128(2));
        assert_eq!(
            scope_pills(&rows, "environment", None),
            vec![(env, format!("Environment {}", Uuid::from_u128(2)), 2)]
        );
        assert_eq!(
            scope_pills(&rows, "environment", Some(env))[0].0,
            Scope::System(Uuid::from_u128(3))
        );
        assert!(scope_pills(&rows, "owner", None).is_empty());
        let mut ambiguous = rows;
        ambiguous[0].environment_ids.push(Uuid::from_u128(9));
        assert_eq!(scope_pills(&ambiguous, "environment", Some(env))[0].2, 1);
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
