//! In-memory evaluation navigation contracts. No refresh persistence is promised.
//!
//! Exact attempt identity gates restoration. Receivers must validate captured
//! membership before presenting evidence; catalog and deployed data are not
//! substitutes for an unavailable historical evaluation.

use crate::api::models::{EvalHistoryItem, EvalQueueItem};
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
            columns: ColumnState::new(count),
            expanded: None,
            issue: None,
            finding: None,
            page: 0,
        }
    }
}

/// Retains the evaluation page and drawer for a receiver round trip.
#[derive(Clone, PartialEq)]
pub struct EvaluationReturn {
    /// Exact attempt to restore through the pinned endpoint.
    pub identity: EvaluationIdentity,
    /// Original summary; never silently replaced by a retry summary.
    pub target: EvaluationTarget,
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
    /// Checks the stored summary before restoring any page or drawer state.
    /// The receiver must still validate the pinned API response independently.
    pub fn matches_commit(&self, commit_id: i32) -> bool {
        let (id, sha, attempt) = match &self.target {
            EvaluationTarget::Queue(item) => {
                (item.commit_id, &item.commit_hash, item.attempt_number)
            }
            EvaluationTarget::History(item) => {
                (item.commit_id, &item.commit_hash, item.attempt_number)
            }
        };
        id == commit_id
            && id == self.identity.commit_id
            && sha == &self.identity.commit_sha
            && attempt == self.identity.attempt_number
            && !self.identity.attempt_id.is_nil()
            && sha.len() == 40
            && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}

/// Keeps page state during an in-memory receiver round trip.
pub static EVALUATION_RETURN: GlobalSignal<Option<EvaluationReturn>> = Signal::global(|| None);
/// Carries an explicit evaluation-evidence request into Compliance.
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
/// Missing elements and document-channel failures leave semantic state intact.
pub async fn restore_dom(value: serde_json::Value) {
    let mut eval = document::eval(
        r#"const state = await dioxus.recv(); requestAnimationFrame(() => { for (const s of state.scrolls || []) { for (const e of document.getElementsByClassName(s.className)) { e.scrollTop = s.top; e.scrollLeft = s.left; } } if (state.window) window.scrollTo(state.window.x, state.window.y); if (state.focus) document.getElementById(state.focus)?.focus({preventScroll:true}); });"#,
    );
    let _ = eval.send(value);
    let _ = eval.await;
}
