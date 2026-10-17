//! Exact evaluation navigation and optional in-memory presentation contracts.
//!
//! Exact attempt identity gates restoration. Receivers must validate captured
//! membership before presenting evidence; catalog and deployed data are not
//! substitutes for an unavailable historical evaluation.

use crate::api::models::{EvalHistoryItem, EvalPolicyMatrixResponse, EvalQueueItem};
use dioxus::prelude::*;
use uuid::Uuid;

/// Identifies the exact evaluation represented by a handoff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationIdentity {
    /// Database commit identity.
    pub commit_id: i32,
    /// Full immutable commit SHA.
    pub commit_sha: String,
    /// Exact retained attempt UUID.
    pub attempt_id: Uuid,
    /// Human-visible attempt number, validated with the UUID.
    pub attempt_number: i32,
}

impl EvaluationIdentity {
    /// Reads exact identity from a pinned response, rejecting route mismatches.
    ///
    /// Missing context, invalid SHA, nil UUID, and nonpositive attempts fail closed.
    pub fn from_response(
        data: &EvalPolicyMatrixResponse,
        commit_id: i32,
        attempt_id: Uuid,
    ) -> Option<Self> {
        let snapshot = data.matrix.as_ref()?;
        let sha = snapshot.commit_sha.as_ref()?;
        (data.commit_id == commit_id
            && commit_id > 0
            && !attempt_id.is_nil()
            && snapshot.evaluation_attempt_id == Some(attempt_id)
            && snapshot.attempt_number > 0
            && sha.len() == 40
            && sha.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| Self {
            commit_id,
            commit_sha: sha.clone(),
            attempt_id,
            attempt_number: snapshot.attempt_number,
        })
    }

    /// Checks the complete server identity, including SHA and attempt number.
    pub fn matches_response(&self, data: &EvalPolicyMatrixResponse) -> bool {
        Self::from_response(data, self.commit_id, self.attempt_id).as_ref() == Some(self)
    }
}

/// Identifies shareable historical evidence without relying on browser globals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationEvidenceRoute {
    /// Database commit identity from the path.
    pub commit_id: i32,
    /// Exact retained attempt UUID from the path.
    pub attempt_id: Uuid,
    /// Captured bundle lineage UUID from the query.
    pub bundle_id: Uuid,
    /// Exact bundle version UUID from the query.
    pub bundle_version_id: Uuid,
    /// Captured system UUID from the query.
    pub system_id: Uuid,
    /// Exact policy version UUID from the query; the API supplies its lineage.
    pub policy_version_id: Uuid,
}

impl EvaluationEvidenceRoute {
    /// Parses required full UUIDs and rejects incomplete or nil route targets.
    ///
    /// Returns `None` for a nonpositive commit ID, nil attempt, or malformed,
    /// abbreviated, or nil query UUID. `policy` identifies a policy version.
    pub fn parse(
        commit_id: i32,
        attempt_id: Uuid,
        bundle: &str,
        version: &str,
        system: &str,
        policy: &str,
    ) -> Option<Self> {
        let full_uuid = |value: &str| {
            let parsed = Uuid::parse_str(value).ok()?;
            (value.len() == 36 && !parsed.is_nil()).then_some(parsed)
        };
        if commit_id <= 0 || attempt_id.is_nil() {
            return None;
        }
        Some(Self {
            commit_id,
            attempt_id,
            bundle_id: full_uuid(bundle)?,
            bundle_version_id: full_uuid(version)?,
            system_id: full_uuid(system)?,
            policy_version_id: full_uuid(policy)?,
        })
    }

    /// Resolves policy lineage only from the exact authorized attempt response.
    ///
    /// Returns `None` for mismatched attempt identity or missing or ambiguous
    /// policy-version metadata. This does not establish bundle membership.
    /// The receiver must also validate the system's captured bundle contribution.
    pub fn resolve(&self, data: &EvalPolicyMatrixResponse) -> Option<EvaluationEvidence> {
        let evaluation = EvaluationIdentity::from_response(data, self.commit_id, self.attempt_id)?;
        let snapshot = data.matrix.as_ref()?;
        let mut policies = snapshot
            .columns
            .iter()
            .filter(|p| p.policy_version_id == self.policy_version_id);
        let policy = policies.next()?;
        if policies.next().is_some() {
            return None;
        }
        Some(EvaluationEvidence {
            evaluation,
            system_id: self.system_id,
            bundle_id: self.bundle_id,
            bundle_version_id: self.bundle_version_id,
            policy_id: policy.policy_id,
            policy_version_id: self.policy_version_id,
        })
    }
}

/// Selects Policy for retained attempts because exact Log/Graph APIs are absent.
/// The original return-state tab remains stored for a future supported receiver.
pub fn retained_tab(tab: &str, pinned: bool) -> String {
    if pinned { "policy".into() } else { tab.into() }
}

/// Selects evaluation check evidence, not framework satisfaction or a finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationEvidence {
    /// Exact evaluation identity.
    pub evaluation: EvaluationIdentity,
    /// Captured system UUID.
    pub system_id: Uuid,
    /// Captured bundle lineage UUID.
    pub bundle_id: Uuid,
    /// Captured bundle version UUID.
    pub bundle_version_id: Uuid,
    /// Captured policy lineage UUID.
    pub policy_id: Uuid,
    /// Captured policy version UUID.
    pub policy_version_id: Uuid,
}

/// Stores the originating summary without importing private view types.
#[derive(Clone, PartialEq)]
pub enum EvaluationTarget {
    /// Active queue summary.
    Queue(EvalQueueItem),
    /// Historical summary.
    History(EvalHistoryItem),
}

/// Selects a mutually exclusive system health filter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowFilter {
    /// Includes all planned systems.
    #[default]
    All,
    /// Includes systems with a known failure.
    Failing,
    /// Includes warnings without failures.
    Warning,
    /// Includes fully passing, nonempty systems.
    Clean,
    /// Includes empty or incomplete systems without known issues.
    Unavailable,
}

/// Selects captured bundle rollups or exact policy-version columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnMode {
    /// Shows bundle rollups and explicit synthetic groups.
    Bundles,
    /// Shows exact controls, optionally focused on one bundle.
    Controls,
}

/// Selects system ordering independently of column mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowSort {
    /// Shows the most severe issues first.
    #[default]
    WorstFirst,
    /// Orders by captured system name.
    Name,
}

/// Stores typed matrix column, filter, and ordering choices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnState {
    /// Current column unit.
    pub mode: ColumnMode,
    /// Current row filter.
    pub filter: RowFilter,
    /// Current system ordering.
    pub sort: RowSort,
    /// Exact bundle version focus, validated against the capture by the view.
    pub focused_bundle: Option<String>,
    /// Trims columns using non-pass evidence, including unknown outcomes.
    pub trim: bool,
}

impl ColumnState {
    /// Creates design defaults using the strictly-greater-than-14 dense threshold.
    pub fn new(unique_control_count: usize) -> Self {
        let dense = unique_control_count > 14;
        Self {
            mode: if dense {
                ColumnMode::Bundles
            } else {
                ColumnMode::Controls
            },
            filter: RowFilter::All,
            sort: RowSort::WorstFirst,
            focused_bundle: None,
            trim: dense,
        }
    }

    /// Selects a column mode and clears bundle focus, preserving other choices.
    pub fn set_mode(&mut self, mode: ColumnMode) {
        self.mode = mode;
        self.focused_bundle = None;
    }

    /// Focuses one exact captured bundle's controls, preserving other choices.
    pub fn focus_bundle(&mut self, bundle_id: &str) {
        self.mode = ColumnMode::Controls;
        self.focused_bundle = Some(bundle_id.to_owned());
    }
}

/// Retains matrix presentation independently of request and component lifetime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatrixPresentation {
    /// Distinguishes usable-capture defaults from a provisional empty response.
    pub initialized: bool,
    /// Typed mode, filter, sort, trim, and exact bundle focus.
    pub columns: ColumnState,
    /// Expanded system/configuration key.
    pub expanded: Option<String>,
    /// Exact policy-version issue filter.
    pub issue: Option<String>,
    /// Open assertion card identity.
    pub finding: Option<String>,
    /// Current bounded row page.
    pub page: usize,
}

impl MatrixPresentation {
    /// Creates design defaults from the deduplicated control count.
    pub fn new(count: usize) -> Self {
        Self {
            initialized: false,
            columns: ColumnState::new(count),
            expanded: None,
            issue: None,
            finding: None,
            page: 0,
        }
    }

    /// Initializes density once captured controls or authoritative emptiness exist.
    /// Subsequent refreshes preserve explicit presentation choices.
    pub fn initialize(
        &mut self,
        snapshot: &crate::api::models::eval_policy_matrix::EvalPolicyMatrixSnapshot,
    ) {
        use crate::api::models::eval_policy_matrix::EvalMatrixProvenance;
        let count = snapshot
            .columns
            .iter()
            .map(|column| column.policy_version_id)
            .collect::<std::collections::HashSet<_>>()
            .len();
        if !self.initialized
            && snapshot.provenance == EvalMatrixProvenance::Captured
            && (count > 0 || snapshot.empty_bundle_metadata_complete)
        {
            self.columns = ColumnState::new(count);
            self.initialized = true;
        }
    }
}

/// Retains the evaluation page and drawer for a receiver round trip.
#[derive(Clone, PartialEq)]
pub struct EvaluationReturn {
    /// Exact attempt to restore through the pinned endpoint.
    pub identity: EvaluationIdentity,
    /// Original summary; absent for direct URLs without a queue/history origin.
    /// Receivers must not substitute a latest summary when this is absent.
    pub target: Option<EvaluationTarget>,
    /// History page selection.
    pub history: bool,
    /// Search text.
    pub search: String,
    /// History status filter.
    pub status: String,
    /// History flake filter.
    pub flake: String,
    /// Latest-only page state.
    pub latest: bool,
    /// Selected drawer tab.
    pub tab: String,
    /// Expanded drawer size.
    pub maximized: bool,
    /// Exact system receiver context, if opened.
    pub system: Option<Uuid>,
    /// Captured scroll positions and invoking element ID.
    pub dom: Option<serde_json::Value>,
}

impl EvaluationReturn {
    /// Requires both path identities before reusing rich presentation state.
    pub fn matches_route(&self, commit_id: i32, attempt_id: Uuid) -> bool {
        self.matches_commit(commit_id) && self.identity.attempt_id == attempt_id
    }

    /// Checks the stored summary before restoring any page or drawer state.
    /// The receiver must still validate the pinned API response independently.
    pub fn matches_commit(&self, commit_id: i32) -> bool {
        let (id, sha, attempt) = match &self.target {
            Some(EvaluationTarget::Queue(item)) => {
                (item.commit_id, &item.commit_hash, item.attempt_number)
            }
            Some(EvaluationTarget::History(item)) => {
                (item.commit_id, &item.commit_hash, item.attempt_number)
            }
            None => (
                self.identity.commit_id,
                &self.identity.commit_sha,
                self.identity.attempt_number,
            ),
        };
        id == commit_id
            && id == self.identity.commit_id
            && sha == &self.identity.commit_sha
            && attempt == self.identity.attempt_number
            && attempt > 0
            && id > 0
            && !self.identity.attempt_id.is_nil()
            && sha.len() == 40
            && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}

impl EvaluationReturn {
    /// Creates presentation defaults for an exact URL without a saved summary.
    ///
    /// Only the pinned API can supply identity; no queue/history data is inferred.
    pub fn direct(identity: EvaluationIdentity) -> Self {
        Self {
            identity,
            target: None,
            history: true,
            search: String::new(),
            status: "all".into(),
            flake: "all".into(),
            latest: false,
            tab: "policy".into(),
            maximized: false,
            system: None,
            dom: None,
        }
    }
}

/// Keeps page state during an in-memory receiver round trip.
pub static EVALUATION_RETURN: GlobalSignal<Option<EvaluationReturn>> = Signal::global(|| None);
/// Carries optional legacy handoff context; exact routes do not require it.
pub static EVALUATION_EVIDENCE: GlobalSignal<Option<EvaluationEvidence>> = Signal::global(|| None);
/// Retains presentation by exact commit and attempt, including across errors.
pub static MATRIX_PRESENTATIONS: GlobalSignal<
    std::collections::HashMap<(i32, Uuid), MatrixPresentation>,
> = Signal::global(std::collections::HashMap::new);

/// Captures scroll and invoking focus without interpolating data into JavaScript.
/// Returns `None` when the document evaluation channel is unavailable.
pub async fn capture_dom() -> Option<serde_json::Value> {
    document::eval(r#"return {window: {x: scrollX, y: scrollY}, scrolls: [...document.querySelectorAll('.pm-scroll, .ed-body, .page-content, .main')].map(e => ({className: e.className, top:e.scrollTop, left:e.scrollLeft})), focus: document.activeElement?.id || null};"#)
        .await.ok()
}

/// Restores captured DOM state through a JSON channel after the drawer mounts.
/// Allows three seconds for the validated matrix's asynchronous render, without
/// moving focus after new operator input. Missing elements and document-channel
/// failures leave semantic state intact.
pub async fn restore_dom(value: serde_json::Value) {
    let mut eval = document::eval(
        r#"
        const state = await dioxus.recv();
        const deadline = performance.now() + 3000;
        let interrupted = false;
        const interrupt = () => { interrupted = true; };
        document.addEventListener('pointerdown', interrupt, true);
        document.addEventListener('keydown', interrupt, true);
        try {
            while (!interrupted && performance.now() < deadline) {
                await new Promise(resolve => requestAnimationFrame(resolve));
                if (interrupted) break;
                const focus = state.focus ? document.getElementById(state.focus) : null;
                if (state.focus && !focus) continue;
                const targets = (state.scrolls || []).flatMap(saved =>
                    [...document.getElementsByClassName(saved.className)]
                        .map(element => ({element, saved})));
                if ((state.scrolls || []).length && !targets.length) continue;
                for (const {element, saved} of targets) {
                    element.scrollTop = saved.top;
                    element.scrollLeft = saved.left;
                }
                if (state.window) window.scrollTo(state.window.x, state.window.y);
                focus?.focus({preventScroll:true});
                await new Promise(resolve => requestAnimationFrame(resolve));
                const restored = targets.every(({element, saved}) =>
                    Math.abs(element.scrollTop - Math.min(saved.top, element.scrollHeight - element.clientHeight)) <= 1 &&
                    Math.abs(element.scrollLeft - Math.min(saved.left, element.scrollWidth - element.clientWidth)) <= 1);
                if (restored && (!focus || document.activeElement === focus)) break;
            }
        } finally {
            document.removeEventListener('pointerdown', interrupt, true);
            document.removeEventListener('keydown', interrupt, true);
        }
        "#,
    );
    let _ = eval.send(value);
    let _ = eval.await;
}

#[cfg(test)]
mod navigation_tests {
    use super::*;

    fn capture(
        count: usize,
        complete: bool,
    ) -> crate::api::models::eval_policy_matrix::EvalPolicyMatrixSnapshot {
        serde_json::from_value(serde_json::json!({
            "evaluation_attempt_id": Uuid::from_u128(1), "attempt_number": 1,
            "provenance": "captured", "empty_bundle_metadata_complete": complete,
            "columns": (0..count).map(|n| serde_json::json!({"policy_id":Uuid::from_u128(n as u128 + 1), "policy_version_id":Uuid::from_u128(n as u128 + 1), "name":"control", "version":"1"})).collect::<Vec<_>>(),
            "bundles":[], "rows":[]
        })).unwrap()
    }

    #[test]
    fn density_waits_for_usable_capture_then_preserves_explicit_choices() {
        let mut state = MatrixPresentation::new(0);
        state.initialize(&capture(0, false));
        assert!(!state.initialized);
        state.initialize(&capture(128, true));
        assert!(state.initialized);
        assert_eq!(state.columns.mode, ColumnMode::Bundles);
        assert!(state.columns.trim);
        state.columns.set_mode(ColumnMode::Controls);
        state.columns.trim = false;
        state.columns.filter = RowFilter::Failing;
        state.columns.sort = RowSort::Name;
        let chosen = state.clone();
        state.initialize(&capture(128, true));
        assert_eq!(state, chosen);
        let mut new_attempt = MatrixPresentation::new(0);
        new_attempt.initialize(&capture(128, true));
        assert_eq!(new_attempt.columns.mode, ColumnMode::Bundles);
    }

    #[test]
    fn density_deduplicates_versions_and_accepts_authoritative_empty_capture() {
        let mut snapshot = capture(14, true);
        snapshot.columns.extend(snapshot.columns.clone());
        let mut state = MatrixPresentation::new(0);
        state.initialize(&snapshot);
        assert_eq!(state.columns.mode, ColumnMode::Controls);
        assert!(!state.columns.trim);
        let mut empty = MatrixPresentation::new(0);
        empty.initialize(&capture(0, true));
        assert!(empty.initialized);
        assert_eq!(empty.columns.mode, ColumnMode::Controls);
    }

    #[test]
    fn evidence_routes_require_full_non_nil_identity() {
        let id = Uuid::from_u128(1);
        let full = id.to_string();
        assert!(EvaluationEvidenceRoute::parse(1, id, &full, &full, &full, &full).is_some());
        for invalid in [
            "",
            "1",
            "00000000000000000000000000000001",
            "00000000-0000-0000-0000-000000000000",
        ] {
            assert!(EvaluationEvidenceRoute::parse(1, id, invalid, &full, &full, &full).is_none());
            assert!(EvaluationEvidenceRoute::parse(1, id, &full, invalid, &full, &full).is_none());
            assert!(EvaluationEvidenceRoute::parse(1, id, &full, &full, invalid, &full).is_none());
            assert!(EvaluationEvidenceRoute::parse(1, id, &full, &full, &full, invalid).is_none());
        }
        assert!(EvaluationEvidenceRoute::parse(0, id, &full, &full, &full, &full).is_none());
        assert!(
            EvaluationEvidenceRoute::parse(1, Uuid::nil(), &full, &full, &full, &full).is_none()
        );
    }

    #[test]
    fn direct_return_cannot_match_another_commit_or_attempt() {
        let identity = EvaluationIdentity {
            commit_id: 1,
            commit_sha: "a".repeat(40),
            attempt_id: Uuid::from_u128(1),
            attempt_number: 2,
        };
        let saved = EvaluationReturn::direct(identity.clone());
        assert!(saved.target.is_none());
        assert!(saved.matches_route(1, identity.attempt_id));
        assert!(!saved.matches_route(2, identity.attempt_id));
        assert!(!saved.matches_route(1, Uuid::from_u128(2)));
        let mut invalid = saved.clone();
        invalid.identity.commit_sha = "latest".into();
        assert!(!invalid.matches_commit(1));
        invalid = saved.clone();
        invalid.identity.attempt_id = Uuid::nil();
        assert!(!invalid.matches_commit(1));
        invalid = saved;
        invalid.identity.attempt_number = 0;
        assert!(!invalid.matches_commit(1));
    }

    #[test]
    fn retained_tabs_select_policy_without_mutating_original_return_choice() {
        for tab in ["log", "graph", "policy"] {
            assert_eq!(retained_tab(tab, true), "policy");
            assert_eq!(retained_tab(tab, false), tab);
        }
    }

    #[test]
    fn rich_return_rejects_summary_identity_drift_and_preserves_presentation() {
        let identity = EvaluationIdentity {
            commit_id: 1,
            commit_sha: "a".repeat(40),
            attempt_id: Uuid::from_u128(1),
            attempt_number: 2,
        };
        let summary: EvalQueueItem = serde_json::from_value(serde_json::json!({
            "commit_id":1, "flake_id":1, "flake_name":"captured", "branch":"main",
            "commit_hash":identity.commit_sha, "commit_message":null, "author":null,
            "committed_at":"2026-10-16T00:00:00Z", "enqueued_at":"2026-10-16T00:00:00Z",
            "evaluation_status":"in_progress", "queue_position":1, "systems":[],
            "system_count":0, "passed_count":0, "policy_failed_count":0, "eval_failed_count":0,
            "attempt_number":2
        }))
        .unwrap();
        let mut saved = EvaluationReturn::direct(identity.clone());
        saved.target = Some(EvaluationTarget::Queue(summary.clone()));
        saved.search = "host".into();
        saved.status = "failed".into();
        saved.flake = "7".into();
        saved.tab = "graph".into();
        saved.maximized = true;
        saved.dom = Some(serde_json::json!({"focus":"invoking-control", "window":{"x":0,"y":90}}));
        let original = saved.clone();
        assert!(saved.matches_route(1, identity.attempt_id));
        assert_eq!(retained_tab(&saved.tab, true), "policy");
        assert!(saved == original);
        for index in 0..3 {
            let mut wrong = summary.clone();
            match index {
                0 => wrong.commit_id += 1,
                1 => wrong.commit_hash = "b".repeat(40),
                _ => wrong.attempt_number += 1,
            }
            saved.target = Some(EvaluationTarget::Queue(wrong));
            assert!(!saved.matches_route(1, identity.attempt_id));
        }
    }
}
