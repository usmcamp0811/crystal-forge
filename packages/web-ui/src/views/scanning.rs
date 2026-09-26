use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use uuid::Uuid;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, closure::Closure};

use crate::api::client::{
    ApiClientError, fetch_environments, fetch_scanning_scan_detail, fetch_scanning_scan_records,
    fetch_scanning_scan_records_with_timeout, fetch_scanning_schedule, fetch_scanning_stats,
    fetch_scanning_system_scans, fetch_scanning_systems, trigger_cve_derivation_rescan,
    update_scanning_archive_state, update_scanning_schedule,
};
use crate::api::models::{
    ScanRecordCollectionParam, ScanRecordDirectionParam, ScanRecordRevisionParam,
    ScanRecordSortParam, ScanRecordStatusParam, ScanSchedulePolicyResponse,
    ScanningQueueItemResponse, ScanningScanDetailResponse, ScanningScanRecordQuery,
    ScanningScanRecordResponse, ScanningScanRecordsResponse, ScanningSystemsItemResponse,
    UpdateScanSchedulePolicyRequest,
};
use crate::components::chips::EnvBadge;
use crate::components::dialog_focus::{
    DialogFocusBoundary, DialogFocusRestore, DialogFocusSentinel, DialogInitialFocus,
};
use crate::components::icon::{Icon, IconName};
use crate::hooks::{InfiniteScroll, use_infinite_scroll};

/// Bounds one Completed page request.
///
/// The server issues a continuation cursor whenever more matching rows exist,
/// so this value is a page size and not a collection cap.
const COMPLETED_PAGE_LIMIT: u16 = 50;

/// Bounds the single Active request.
///
/// The server rejects continuation cursors for nonterminal collections because
/// waiting, queued, and running rows have no stable keyset order. Active
/// therefore loads one explicitly bounded page and discloses when the server
/// reports more matching rows than are loaded.
const ACTIVE_PAGE_LIMIT: u16 = 200;

/// Bounds one per-system history request.
///
/// The history collection mixes nonterminal and terminal rows and has the same
/// cursor restriction as [`ACTIVE_PAGE_LIMIT`].
const SYSTEM_HISTORY_LIMIT: u16 = 200;

/// Bounds the per-system derivation projection request.
const SYSTEM_DERIVATION_LIMIT: i64 = 500;

/// Bounds the fleet system projection request.
const SYSTEM_LIMIT: i64 = 500;

/// Bounds one archive or restore request to the server's exact-identity limit.
const ARCHIVE_BATCH_MAX: usize = 100;

/// Applies one Completed-row selection gesture against the currently loaded
/// display order.
///
/// Shift selection adds the inclusive range to existing selections and keeps
/// the original anchor. If the anchor is absent or no longer loaded, Shift
/// selects only the clicked row and makes that row the new anchor. Ctrl/Cmd
/// toggles only the clicked identity; a checkbox applies its new checked state.
/// Both single-row gestures set the clicked row as anchor. This helper never
/// considers rows outside `ordered_ids`, including unloaded keyset pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompletedSelectionGesture {
    ShiftRange,
    ModifierToggle,
    Checkbox { checked: bool },
}

fn completed_range_selection(
    selected: &HashSet<Uuid>,
    anchor: Option<Uuid>,
    clicked: Uuid,
    ordered_ids: &[Uuid],
    gesture: CompletedSelectionGesture,
) -> (HashSet<Uuid>, Option<Uuid>) {
    let mut next = selected.clone();
    let Some(clicked_index) = ordered_ids.iter().position(|id| *id == clicked) else {
        return (next, anchor.filter(|id| ordered_ids.contains(id)));
    };

    match gesture {
        CompletedSelectionGesture::ShiftRange => {
            if let Some(anchor) = anchor
                && let Some(anchor_index) = ordered_ids.iter().position(|id| *id == anchor)
            {
                let start = anchor_index.min(clicked_index);
                let end = anchor_index.max(clicked_index);
                next.extend(ordered_ids[start..=end].iter().copied());
                (next, Some(anchor))
            } else {
                next.insert(clicked);
                (next, Some(clicked))
            }
        }
        CompletedSelectionGesture::ModifierToggle => {
            if !next.remove(&clicked) {
                next.insert(clicked);
            }
            (next, Some(clicked))
        }
        CompletedSelectionGesture::Checkbox { checked: true } => {
            next.insert(clicked);
            (next, Some(clicked))
        }
        CompletedSelectionGesture::Checkbox { checked: false } => {
            next.remove(&clicked);
            (next, Some(clicked))
        }
    }
}

const DETAIL_POLL_MS: u32 = 3_000;
const LIVE_REFRESH_MS: u32 = 15_000;
/// Releases a live-refresh slot before the next polling interval.
const HEAD_REFRESH_TIMEOUT_MS: u32 = 10_000;

/// Delays server search requests until typing pauses.
const SEARCH_DEBOUNCE_MS: u32 = 250;

// INVARIANT: the server validates every scan-record page size against 1..=500
// and answers any other value with a validation error. The client sends these
// constants unchanged instead of clamping, so an out-of-range page size is a
// build-time failure rather than a silently truncated collection.
const _: () = assert!(COMPLETED_PAGE_LIMIT >= 1 && COMPLETED_PAGE_LIMIT <= 500);
const _: () = assert!(ACTIVE_PAGE_LIMIT >= 1 && ACTIVE_PAGE_LIMIT <= 500);
const _: () = assert!(SYSTEM_HISTORY_LIMIT >= 1 && SYSTEM_HISTORY_LIMIT <= 500);

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanTab {
    Active,
    Completed,
    Systems,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanDetailTab {
    Log,
    Details,
}

/// Orders the single loaded Active page inside the browser.
///
/// The server does not order nonterminal collections by request, so this key
/// only applies to rows that are already loaded. The Active panel offers it
/// only while the loaded page holds the complete matching collection.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanSort {
    Configuration,
    Revision,
    Status,
    Severity,
    Timestamp,
}

/// Identifies one in-flight scan-record continuation request.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ScanRecordContinuation {
    generation: u64,
    /// Contains the opaque server-issued cursor for this request.
    cursor: String,
}

/// Identifies one in-flight head-page refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScanRecordHeadRefresh {
    generation: u64,
    sequence: u64,
}

/// Selects how a head-page refresh treats rows that are not loaded yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadRefreshMode {
    /// Inserts unseen head rows before the loaded rows. Only newest-first
    /// ordering guarantees that a new terminal row belongs at the head.
    PrependNewRows,
    /// Updates loaded rows only. Any other ordering could place an unseen row
    /// between loaded rows, which would reorder rows the operator can see.
    UpdateLoadedOnly,
}

/// Accumulates server-issued scan-record pages for one request identity.
///
/// The state rejects responses from superseded requests, keeps loaded rows
/// visible when a continuation fails, and never lets a live refresh
/// re-request a page that is already loaded.
#[derive(Debug, Clone, Default, PartialEq)]
struct ScanRecordPaginationState {
    /// Contains the accumulated rows and the newest page metadata.
    page: Option<ScanningScanRecordsResponse>,
    /// Reports whether one continuation request is active.
    continuation_loading: bool,
    /// Contains the latest continuation failure while loaded rows remain.
    continuation_error: Option<String>,
    /// Counts loaded server pages, including the head page.
    pages_loaded: usize,
    generation: u64,
    /// Identifies the newest head refresh started in this generation.
    head_refresh_sequence: u64,
    /// Is true while one head refresh owns the current sequence.
    head_refresh_loading: bool,
}

impl ScanRecordPaginationState {
    /// Replaces every loaded page and invalidates all earlier requests.
    ///
    /// CONCURRENCY: the generation bump makes every in-flight continuation and
    /// head refresh for a previous request identity a no-op on completion.
    fn reset(&mut self, page: Option<ScanningScanRecordsResponse>) {
        self.generation = self.generation.wrapping_add(1);
        self.pages_loaded = usize::from(page.is_some());
        self.page = page;
        self.continuation_loading = false;
        self.continuation_error = None;
        self.head_refresh_sequence = self.head_refresh_sequence.wrapping_add(1);
        self.head_refresh_loading = false;
    }

    /// Returns the accumulated rows in the server's collection order.
    fn rows(&self) -> &[ScanningScanRecordResponse] {
        self.page.as_ref().map_or(&[], |page| page.items.as_slice())
    }

    /// Is true when the head page has been loaded.
    fn is_loaded(&self) -> bool {
        self.page.is_some()
    }

    /// Returns the current request generation.
    ///
    /// The value changes on every reset, so callers can tie scroll paging to
    /// the loaded pages and start again after a reload.
    fn generation(&self) -> u64 {
        self.generation
    }

    /// Is true when the server offers another requestable page.
    fn has_more(&self) -> bool {
        self.page
            .as_ref()
            .is_some_and(|page| page.has_more && page.next_cursor.is_some())
    }

    /// Counts matching rows the server reports before archive filtering.
    fn total(&self) -> i64 {
        self.page.as_ref().map_or(0, |page| page.total)
    }

    /// Counts archived matching rows the current archive filter hides.
    fn hidden_archived(&self) -> i64 {
        self.page
            .as_ref()
            .map_or(0, |page| page.hidden_archived.max(0))
    }

    /// Counts matching rows the current archive filter shows.
    fn visible_total(&self) -> i64 {
        (self.total() - self.hidden_archived()).max(0)
    }

    /// Starts one continuation request when no other page request is active.
    fn begin_continuation(&mut self) -> Option<ScanRecordContinuation> {
        if self.continuation_loading || self.head_refresh_loading {
            return None;
        }
        let page = self.page.as_ref()?;
        if !page.has_more {
            return None;
        }
        let cursor = page.next_cursor.clone()?;
        self.continuation_loading = true;
        self.continuation_error = None;
        Some(ScanRecordContinuation {
            generation: self.generation,
            cursor,
        })
    }

    /// Appends a matching continuation page and ignores stale completions.
    ///
    /// Returns `false` when no head page remains, which requires the caller to
    /// request a new first page.
    fn complete_continuation(
        &mut self,
        request: &ScanRecordContinuation,
        page: ScanningScanRecordsResponse,
    ) -> bool {
        if request.generation != self.generation || !self.continuation_loading {
            return true;
        }
        self.continuation_loading = false;
        let Some(current) = self.page.as_mut() else {
            return false;
        };
        // INVARIANT: keyset pages never repeat a scan identity, but a
        // concurrent archive or restore can move a page boundary. Deduplicating
        // by scan identity keeps every row unique without dropping loaded rows.
        let mut identities = current
            .items
            .iter()
            .map(|row| row.scan_id)
            .collect::<HashSet<_>>();
        current.items.extend(
            page.items
                .into_iter()
                .filter(|row| identities.insert(row.scan_id)),
        );
        // The cursor binds this page to the high-water snapshot captured by the
        // original head request. A later live head refresh can report a newer
        // total, so continuation metadata must not replace the live total.
        current.has_more = page.has_more;
        current.next_cursor = page.next_cursor;
        self.pages_loaded = self.pages_loaded.saturating_add(1);
        true
    }

    /// Records a matching continuation failure and preserves loaded rows.
    fn fail_continuation(&mut self, request: &ScanRecordContinuation, message: String) {
        if request.generation == self.generation && self.continuation_loading {
            self.continuation_loading = false;
            self.continuation_error = Some(message);
        }
    }

    /// Starts one head-page refresh for the loaded request identity.
    ///
    /// Returns `None` before the head page exists or while another page request
    /// is active, because concurrent merges could duplicate or reorder rows.
    fn begin_head_refresh(&mut self) -> Option<ScanRecordHeadRefresh> {
        if self.page.is_none() || self.continuation_loading || self.head_refresh_loading {
            return None;
        }
        self.head_refresh_sequence = self.head_refresh_sequence.wrapping_add(1);
        self.head_refresh_loading = true;
        Some(ScanRecordHeadRefresh {
            generation: self.generation,
            sequence: self.head_refresh_sequence,
        })
    }

    /// Applies a matching head refresh without re-requesting later pages.
    ///
    /// INVARIANT: a continuation cursor carries the high-water bound captured
    /// by the first request of this identity. Once a continuation page is
    /// loaded, the refreshed head page's own cursor must be ignored; adopting
    /// it would re-request rows that are already visible.
    ///
    /// Returns `false` when the response belongs to a superseded request.
    fn complete_head_refresh(
        &mut self,
        request: &ScanRecordHeadRefresh,
        head: ScanningScanRecordsResponse,
        mode: HeadRefreshMode,
    ) -> bool {
        if request.generation != self.generation || request.sequence != self.head_refresh_sequence {
            return false;
        }
        self.head_refresh_loading = false;
        let Some(current) = self.page.as_mut() else {
            return false;
        };
        if self.pages_loaded <= 1 {
            // One loaded page is exactly the head page, so the fresh response
            // replaces it, cursor included.
            *current = head;
            self.pages_loaded = 1;
            return true;
        }
        let total_delta = head.total - current.total;
        let loaded = current
            .items
            .iter()
            .map(|row| row.scan_id)
            .collect::<HashSet<_>>();
        let discovered_new_rows = head
            .items
            .iter()
            .filter(|row| !loaded.contains(&row.scan_id))
            .count();
        let refresh_exceeds_old_cursor = total_delta < 0
            || head.hidden_archived != current.hidden_archived
            || (total_delta > 0
                && (mode == HeadRefreshMode::UpdateLoadedOnly
                    || total_delta as usize > discovered_new_rows));
        if refresh_exceeds_old_cursor {
            // The continuation cursor belongs to the old high-water snapshot.
            // Restart from the refreshed head when rows can fall outside that
            // cursor; otherwise the UI can report the new total while making
            // some matching rows permanently unreachable.
            *current = head;
            self.pages_loaded = 1;
            return true;
        }
        let refreshed = head
            .items
            .iter()
            .map(|row| (row.scan_id, row))
            .collect::<HashMap<_, _>>();
        for row in current.items.iter_mut() {
            if let Some(updated) = refreshed.get(&row.scan_id) {
                *row = (*updated).clone();
            }
        }
        if mode == HeadRefreshMode::PrependNewRows {
            let mut merged = head
                .items
                .into_iter()
                .filter(|row| !loaded.contains(&row.scan_id))
                .collect::<Vec<_>>();
            merged.append(&mut current.items);
            current.items = merged;
        }
        current.total = head.total;
        current.hidden_archived = head.hidden_archived;
        true
    }

    /// Releases the matching refresh slot after a request failure.
    fn fail_head_refresh(&mut self, request: &ScanRecordHeadRefresh) {
        if request.generation == self.generation && request.sequence == self.head_refresh_sequence {
            self.head_refresh_loading = false;
        }
    }
}

/// Identifies one exact Completed request sent to the server.
///
/// The server's cursor fingerprint covers every value here, so any change must
/// restart pagination from the first page without a cursor.
#[derive(Clone, PartialEq, Eq)]
struct CompletedRequest {
    include_archived: bool,
    search: String,
    status: ScanRecordStatusParam,
    revision: ScanRecordRevisionParam,
    latest_only: bool,
    sort: ScanRecordSortParam,
    direction: ScanRecordDirectionParam,
}

impl CompletedRequest {
    /// Builds the head or continuation query for this request identity.
    fn to_query(&self, after: Option<String>) -> ScanningScanRecordQuery {
        ScanningScanRecordQuery {
            collection: ScanRecordCollectionParam::Completed,
            include_archived: self.include_archived,
            system_id: None,
            limit: COMPLETED_PAGE_LIMIT,
            search: (!self.search.is_empty()).then(|| self.search.clone()),
            status: self.status,
            revision: self.revision,
            latest_only: self.latest_only,
            sort: self.sort,
            direction: self.direction,
            after,
        }
    }

    /// Builds the bounded archive-count probe for the Archived badge.
    ///
    /// The probe repeats the current filters with archived rows excluded and a
    /// one-row page, so `hidden_archived` counts every archived row the
    /// current view hides without transferring those rows.
    fn archived_count_query(&self) -> ScanningScanRecordQuery {
        ScanningScanRecordQuery {
            include_archived: false,
            limit: 1,
            ..self.to_query(None)
        }
    }

    /// Returns the infinite-scroll reset key for this request identity.
    fn reset_key(&self) -> String {
        format!(
            "completed|{}|{}|{}|{}|{}|{}|{}",
            self.include_archived,
            self.search,
            self.status.as_param(),
            self.revision.as_param(),
            self.latest_only,
            self.sort.as_param(),
            self.direction.as_param(),
        )
    }

    /// Selects the head-refresh behavior implied by the server ordering.
    fn head_refresh_mode(&self) -> HeadRefreshMode {
        if self.sort == ScanRecordSortParam::Timestamp
            && self.direction == ScanRecordDirectionParam::Desc
        {
            HeadRefreshMode::PrependNewRows
        } else {
            HeadRefreshMode::UpdateLoadedOnly
        }
    }

    /// Is true when the request narrows the collection beyond archive state.
    fn has_narrowing_filter(&self) -> bool {
        !self.search.is_empty()
            || self.status != ScanRecordStatusParam::All
            || self.revision != ScanRecordRevisionParam::All
            || self.latest_only
    }
}

/// Groups the Completed filter signals owned by [`ScanningView`].
#[derive(Clone, Copy)]
struct CompletedFilterSignals {
    include_archived: Signal<bool>,
    /// Contains the raw search input before debouncing.
    query: Signal<String>,
    status: Signal<ScanRecordStatusParam>,
    revision: Signal<ScanRecordRevisionParam>,
    latest_only: Signal<bool>,
    sort: Signal<ScanRecordSortParam>,
    direction: Signal<ScanRecordDirectionParam>,
}

/// Groups the Active filter signals owned by [`ScanningView`].
///
/// These filters apply to the single loaded Active page because the server
/// does not filter or order nonterminal collections by request.
#[derive(Clone, Copy)]
struct ActiveFilterSignals {
    query: Signal<String>,
    status: Signal<String>,
    revision: Signal<String>,
    latest_only: Signal<bool>,
    sort: Signal<ScanSort>,
    descending: Signal<bool>,
}

/// Groups the row-level interaction signals shared by every record panel.
#[derive(Clone, Copy)]
struct RecordRowContext {
    retry_pending: Signal<HashSet<i32>>,
    feedback: Signal<Option<ScanActionFeedback>>,
    refresh: Signal<u64>,
    selected_scan: Signal<Option<ScanDetailSelection>>,
    detail_state: Signal<ScanDetailState>,
    detail_generation: Signal<u64>,
}

/// Carries the Completed collection's loaded-order range selection into each
/// rendered row. The order is copied from the loaded keyset pages; it never
/// requests or infers identities from pages that are not loaded.
#[derive(Clone)]
struct CompletedRowSelection {
    anchor: Signal<Option<Uuid>>,
    ordered_ids: Rc<Vec<Uuid>>,
}

/// Builds the bounded Active collection query.
///
/// CONTRACT: the server applies only `collection`, `system_id`,
/// `include_archived`, and `limit` to nonterminal collections. The neutral
/// filter and ordering values below document that the Active panel asks the
/// server for the whole nonterminal collection and never implies a
/// server-applied filter it cannot get.
fn active_records_query() -> ScanningScanRecordQuery {
    ScanningScanRecordQuery {
        collection: ScanRecordCollectionParam::Active,
        include_archived: false,
        system_id: None,
        limit: ACTIVE_PAGE_LIMIT,
        search: None,
        status: ScanRecordStatusParam::All,
        revision: ScanRecordRevisionParam::All,
        latest_only: false,
        sort: ScanRecordSortParam::Timestamp,
        direction: ScanRecordDirectionParam::Desc,
        after: None,
    }
}

/// Builds the bounded per-system history query for one active system.
fn system_history_query(system_id: Uuid, include_archived: bool) -> ScanningScanRecordQuery {
    ScanningScanRecordQuery {
        collection: ScanRecordCollectionParam::History,
        include_archived,
        system_id: Some(system_id),
        limit: SYSTEM_HISTORY_LIMIT,
        search: None,
        status: ScanRecordStatusParam::All,
        revision: ScanRecordRevisionParam::All,
        latest_only: false,
        sort: ScanRecordSortParam::Timestamp,
        direction: ScanRecordDirectionParam::Desc,
        after: None,
    }
}

/// Builds the server query that resolves the newest failed terminal scan.
///
/// The failed summary action uses this query instead of searching loaded rows,
/// so it stays correct when the Completed view holds a different filter or
/// only part of the collection. Archived rows stay included because archiving
/// hides retained evidence without deleting it.
fn newest_failed_query() -> ScanningScanRecordQuery {
    ScanningScanRecordQuery {
        collection: ScanRecordCollectionParam::Completed,
        include_archived: true,
        system_id: None,
        limit: 1,
        search: None,
        status: ScanRecordStatusParam::Failed,
        revision: ScanRecordRevisionParam::All,
        latest_only: false,
        sort: ScanRecordSortParam::Timestamp,
        direction: ScanRecordDirectionParam::Desc,
        after: None,
    }
}

#[derive(Clone, Copy)]
struct StatusMeta {
    key: &'static str,
    class: &'static str,
    color: &'static str,
    label: &'static str,
}

#[derive(Clone, PartialEq, Eq)]
struct ScanActionFeedback {
    message: String,
    success: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct ScanDetailSelection {
    scan_id: Uuid,
    label: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ScanDetailRequest {
    scan_id: Uuid,
    generation: u64,
}

#[derive(Clone, PartialEq)]
enum ScanDetailState {
    Loading,
    Loaded(ScanningScanDetailResponse),
    Error(String),
}

#[derive(Clone, PartialEq)]
struct SystemHistoryData {
    scans: ScanningScanRecordsResponse,
    derivations: Vec<ScanningQueueItemResponse>,
}

#[derive(Clone, PartialEq)]
enum SystemHistoryEntry {
    Scan(ScanningScanRecordResponse),
    NoScan(ScanningQueueItemResponse),
}

/// Counts the matching rows one collection response shows.
///
/// The server's `total` counts matching rows before archive filtering, so the
/// hidden archived count must be subtracted to describe what the current view
/// contains.
fn visible_record_total(response: &ScanningScanRecordsResponse) -> i64 {
    response.total.saturating_sub(response.hidden_archived)
}

fn system_archive_visibility(states: &HashMap<Uuid, bool>, system_id: Uuid) -> bool {
    states.get(&system_id).copied().unwrap_or(false)
}

fn set_system_archive_visibility(
    states: &mut HashMap<Uuid, bool>,
    system_id: Uuid,
    include_archived: bool,
) {
    if include_archived {
        states.insert(system_id, true);
    } else {
        states.remove(&system_id);
    }
}

fn system_history_entries(data: SystemHistoryData) -> Vec<SystemHistoryEntry> {
    let mut entries = data
        .scans
        .items
        .into_iter()
        .map(SystemHistoryEntry::Scan)
        .collect::<Vec<_>>();
    entries.extend(
        data.derivations
            .into_iter()
            .filter(|row| row.scan_id.is_none())
            .map(SystemHistoryEntry::NoScan),
    );
    entries.sort_by(|left, right| match (left, right) {
        (SystemHistoryEntry::Scan(left), SystemHistoryEntry::Scan(right)) => record_time(right)
            .cmp(&record_time(left))
            .then_with(|| left.scan_id.cmp(&right.scan_id)),
        (SystemHistoryEntry::Scan(_), SystemHistoryEntry::NoScan(_)) => std::cmp::Ordering::Less,
        (SystemHistoryEntry::NoScan(_), SystemHistoryEntry::Scan(_)) => std::cmp::Ordering::Greater,
        (SystemHistoryEntry::NoScan(left), SystemHistoryEntry::NoScan(right)) => right
            .is_current
            .cmp(&left.is_current)
            .then_with(|| right.is_latest_per_flake.cmp(&left.is_latest_per_flake))
            .then_with(|| right.derivation_id.cmp(&left.derivation_id)),
    });
    entries
}

fn status_meta(status: &str) -> StatusMeta {
    match status {
        "in_progress" => StatusMeta {
            key: "in_progress",
            class: "chip-info",
            color: "#60a5fa",
            label: "Scanning",
        },
        "pending" => StatusMeta {
            key: "pending",
            class: "chip-info",
            color: "#a78bfa",
            label: "Queued",
        },
        "awaiting_build" => StatusMeta {
            key: "awaiting_build",
            class: "chip-warning",
            color: "#f59e0b",
            label: "Awaiting build",
        },
        "awaiting_closure" => StatusMeta {
            key: "awaiting_closure",
            class: "chip-unknown",
            color: "#94a3b8",
            label: "Awaiting closure",
        },
        "completed" => StatusMeta {
            key: "completed",
            class: "chip-healthy",
            color: "#34d399",
            label: "Completed",
        },
        "failed" => StatusMeta {
            key: "failed",
            class: "chip-critical",
            color: "#f87171",
            label: "Failed",
        },
        "stale" => StatusMeta {
            key: "stale",
            class: "chip-warning",
            color: "#fbbf24",
            label: "Stale",
        },
        "needs_build" | "needs-build" => StatusMeta {
            key: "needs_build",
            class: "chip-warning",
            color: "#f59e0b",
            label: "Needs build",
        },
        "never_scanned" | "unscanned" => StatusMeta {
            key: "never_scanned",
            class: "chip-unknown",
            color: "#9ca3af",
            label: "Never scanned",
        },
        _ => StatusMeta {
            key: "unknown",
            class: "chip-unknown",
            color: "#9ca3af",
            label: "Unknown",
        },
    }
}

fn status_rank(status: &str) -> u8 {
    match status_meta(status).key {
        "failed" => 0,
        "awaiting_build" => 1,
        "awaiting_closure" => 2,
        "in_progress" => 3,
        "pending" => 4,
        "stale" => 5,
        "completed" => 6,
        "needs_build" => 7,
        "never_scanned" => 8,
        _ => 9,
    }
}

fn record_time(row: &ScanningScanRecordResponse) -> DateTime<Utc> {
    row.completed_at
        .or(row.scheduled_at)
        .unwrap_or(row.created_at)
}

fn severity_counts(row: &ScanningScanRecordResponse) -> (i32, i32, i32, i32) {
    (
        row.critical_count,
        row.high_count,
        row.medium_count,
        row.low_count,
    )
}

fn revision_class(row: &ScanningScanRecordResponse) -> &'static str {
    if row.is_current {
        "deployed"
    } else if row.is_latest_per_flake {
        "recent"
    } else {
        "superseded"
    }
}

fn bounded_failure_preview(value: &str) -> String {
    const MAX_CHARS: usize = 120;
    let mut chars = value.chars();
    let preview = chars.by_ref().take(MAX_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn is_prerequisite_build_failure(row: &ScanningScanRecordResponse) -> bool {
    row.status == "failed"
        && row.source_trigger.as_deref() == Some("post_build")
        && row.attempts == 0
}

/// Filters and orders the single loaded Active page inside the browser.
///
/// The server does not filter or order nonterminal collections by request, so
/// this function only describes rows that are already loaded. The Active panel
/// discloses that bound whenever the server reports more matching rows than the
/// loaded page contains, and it withholds column sorting in that state.
#[allow(clippy::too_many_arguments)]
fn filter_and_sort_active_records(
    rows: &[ScanningScanRecordResponse],
    query: &str,
    status: &str,
    revision: &str,
    latest_only: bool,
    sort: ScanSort,
    descending: bool,
) -> Vec<ScanningScanRecordResponse> {
    let query = query.trim().to_ascii_lowercase();
    let mut filtered = rows
        .iter()
        .filter(|row| {
            let identity = format!(
                "{} {} {} {} {}",
                row.hostname,
                row.flake_name.as_deref().unwrap_or_default(),
                row.commit_hash.as_deref().unwrap_or_default(),
                row.scan_id,
                row.derivation_id
            )
            .to_ascii_lowercase();
            (query.is_empty() || identity.contains(&query))
                && (status == "all" || status_meta(&row.status).key == status)
                && (revision == "all" || revision_class(row) == revision)
                && (!latest_only || row.is_latest_per_flake)
        })
        .cloned()
        .collect::<Vec<_>>();

    filtered.sort_by(|left, right| {
        let order = match sort {
            ScanSort::Configuration => left
                .hostname
                .to_ascii_lowercase()
                .cmp(&right.hostname.to_ascii_lowercase()),
            ScanSort::Revision => left
                .commit_hash
                .as_deref()
                .unwrap_or_default()
                .cmp(right.commit_hash.as_deref().unwrap_or_default()),
            ScanSort::Status => status_rank(&left.status).cmp(&status_rank(&right.status)),
            ScanSort::Severity => severity_counts(left).cmp(&severity_counts(right)),
            ScanSort::Timestamp => record_time(left).cmp(&record_time(right)),
        };
        let order = if descending { order.reverse() } else { order };
        order
            .then_with(|| left.hostname.cmp(&right.hostname))
            .then_with(|| left.commit_hash.cmp(&right.commit_hash))
            .then_with(|| left.scan_id.cmp(&right.scan_id))
    });
    filtered
}

/// Requests the next Completed page and preserves loaded rows on failure.
///
/// A rejected cursor means the request identity no longer matches the loaded
/// pages, so the view discards them and restarts from a fresh first page
/// instead of mixing incompatible pages.
fn request_completed_continuation(
    mut pagination: Signal<ScanRecordPaginationState>,
    mut head: Resource<Result<ScanningScanRecordsResponse, ApiClientError>>,
    request: CompletedRequest,
) {
    let Some(continuation) = pagination.write().begin_continuation() else {
        return;
    };
    let query = request.to_query(Some(continuation.cursor.clone()));
    spawn(async move {
        match fetch_scanning_scan_records(&query).await {
            Ok(page) => {
                if !pagination
                    .write()
                    .complete_continuation(&continuation, page)
                {
                    pagination.write().reset(None);
                    head.restart();
                }
            }
            Err(ApiClientError::Status { code: 400, .. }) => {
                pagination.write().reset(None);
                head.restart();
            }
            Err(error) => pagination
                .write()
                .fail_continuation(&continuation, error.to_string()),
        }
    });
}

/// Opens the newest failed terminal scan reported by the server.
///
/// The lookup requests a one-row failed page, so the summary action never
/// depends on which Completed rows happen to be loaded.
fn open_newest_failed_scan(
    mut pending: Signal<bool>,
    mut feedback: Signal<Option<ScanActionFeedback>>,
    selected: Signal<Option<ScanDetailSelection>>,
    state: Signal<ScanDetailState>,
    generation: Signal<u64>,
) {
    if pending() {
        return;
    }
    pending.set(true);
    feedback.set(None);
    spawn(async move {
        let result = fetch_scanning_scan_records(&newest_failed_query()).await;
        pending.set(false);
        match result {
            Ok(page) => match page.items.first() {
                Some(row) => load_scan_detail(
                    ScanDetailSelection {
                        scan_id: row.scan_id,
                        label: format!("{} · {}", row.hostname, commit_label(&row.commit_hash)),
                    },
                    selected,
                    state,
                    generation,
                ),
                None => feedback.set(Some(ScanActionFeedback {
                    message: "No failed scan is available in the retained history.".to_string(),
                    success: false,
                })),
            },
            Err(error) => feedback.set(Some(ScanActionFeedback {
                message: format!("The newest failed scan could not be loaded: {error}"),
                success: false,
            })),
        }
    });
}

fn scan_detail_request_is_current(
    request: ScanDetailRequest,
    selected: Option<&ScanDetailSelection>,
    generation: u64,
) -> bool {
    request.generation == generation
        && selected.map(|selection| selection.scan_id) == Some(request.scan_id)
}

fn load_scan_detail(
    selection: ScanDetailSelection,
    mut selected: Signal<Option<ScanDetailSelection>>,
    mut state: Signal<ScanDetailState>,
    mut generation: Signal<u64>,
) {
    let request = ScanDetailRequest {
        scan_id: selection.scan_id,
        generation: generation().wrapping_add(1),
    };
    generation.set(request.generation);
    selected.set(Some(selection.clone()));
    state.set(ScanDetailState::Loading);
    spawn(async move {
        let result = fetch_scanning_scan_detail(&selection.scan_id).await;
        if !scan_detail_request_is_current(request, selected.peek().as_ref(), generation()) {
            return;
        }
        state.set(match result {
            Ok(detail) => ScanDetailState::Loaded(detail),
            Err(error) => ScanDetailState::Error(error.to_string()),
        });
    });
}

fn close_scan_detail(
    mut selected: Signal<Option<ScanDetailSelection>>,
    mut generation: Signal<u64>,
) {
    generation.set(generation().wrapping_add(1));
    selected.set(None);
}

fn retry_exact_scan(
    derivation_id: i32,
    label: String,
    mut pending: Signal<HashSet<i32>>,
    mut feedback: Signal<Option<ScanActionFeedback>>,
    mut refresh: Signal<u64>,
) {
    if pending.read().contains(&derivation_id) {
        return;
    }
    pending.write().insert(derivation_id);
    feedback.set(None);
    spawn(async move {
        let result = trigger_cve_derivation_rescan(derivation_id).await;
        pending.write().remove(&derivation_id);
        match result {
            Ok(response) => {
                feedback.set(Some(ScanActionFeedback {
                    message: format!(
                        "{label}: {} exact scan {}.",
                        if response.enqueued {
                            "queued"
                        } else {
                            "reused"
                        },
                        response.scan_id
                    ),
                    success: true,
                }));
                refresh.set(refresh().wrapping_add(1));
            }
            Err(error) => feedback.set(Some(ScanActionFeedback {
                message: format!("{label}: exact retry failed: {error}"),
                success: false,
            })),
        }
    });
}

/// Renders authoritative CVE scan lifecycle administration.
///
/// The view does not expose cancellation because the server reports every scan
/// as non-cancellable. Archive actions only target terminal scan identities.
///
/// The Completed collection is server-filtered, server-ordered, and paged with
/// request-bound keyset cursors. Its search, status, revision, latest-revision,
/// ordering, and archive controls are request parameters, so their result
/// counts describe the whole matching collection instead of the loaded rows.
/// Active and per-system history load one explicitly bounded page each because
/// the server rejects continuation cursors for those collections.
#[component]
pub fn ScanningView() -> Element {
    let mut tab = use_signal(|| ScanTab::Active);
    let refresh = use_signal(|| 0_u64);
    let mut live_refresh = use_signal(|| 0_u64);
    // Each collection owns its filters because the server applies them to the
    // Completed collection only. Sharing them would imply that an Active
    // filter reaches the server.
    let active_filters = ActiveFilterSignals {
        query: use_signal(String::new),
        status: use_signal(|| "all".to_string()),
        revision: use_signal(|| "all".to_string()),
        latest_only: use_signal(|| false),
        sort: use_signal(|| ScanSort::Timestamp),
        descending: use_signal(|| true),
    };
    let completed_filters = CompletedFilterSignals {
        include_archived: use_signal(|| false),
        query: use_signal(String::new),
        status: use_signal(|| ScanRecordStatusParam::All),
        revision: use_signal(|| ScanRecordRevisionParam::All),
        latest_only: use_signal(|| false),
        sort: use_signal(|| ScanRecordSortParam::Timestamp),
        direction: use_signal(|| ScanRecordDirectionParam::Desc),
    };
    let mut completed_search = use_signal(String::new);
    let mut search_debounce = use_signal(|| 0_u64);
    let mut completed_pagination = use_signal(ScanRecordPaginationState::default);
    let mut archived_count = use_signal(|| 0_i64);
    let failed_lookup_pending = use_signal(|| false);
    let mut selected_rows = use_signal(HashSet::<Uuid>::new);
    let mut completed_selection_anchor = use_signal(|| Option::<Uuid>::None);
    let archive_pending = use_signal(|| false);
    let exact_retry_pending = use_signal(HashSet::<i32>::new);
    let mut action_feedback = use_signal(|| Option::<ScanActionFeedback>::None);
    let mut schedule_open = use_signal(|| false);
    let schedule_refresh = use_signal(|| 0_u64);
    let selected_scan = use_signal(|| Option::<ScanDetailSelection>::None);
    let detail_state = use_signal(|| ScanDetailState::Loading);
    let detail_generation = use_signal(|| 0_u64);
    let system_query = use_signal(String::new);
    let system_environment = use_signal(|| "all".to_string());
    let expanded_system = use_signal(|| Option::<Uuid>::None);
    let mut system_histories = use_signal(HashMap::<(Uuid, bool), SystemHistoryData>::new);
    let mut system_errors = use_signal(HashMap::<(Uuid, bool), String>::new);
    let loading_system = use_signal(|| Option::<(Uuid, bool)>::None);
    let system_archive_states = use_signal(HashMap::<Uuid, bool>::new);

    let rows_context = RecordRowContext {
        retry_pending: exact_retry_pending,
        feedback: action_feedback,
        refresh,
        selected_scan,
        detail_state,
        detail_generation,
    };

    // Reads every Completed request value reactively so resources and the
    // infinite-scroll reset key follow filter changes.
    let completed_request = move || CompletedRequest {
        include_archived: *completed_filters.include_archived.read(),
        search: completed_search.read().clone(),
        status: *completed_filters.status.read(),
        revision: *completed_filters.revision.read(),
        latest_only: *completed_filters.latest_only.read(),
        sort: *completed_filters.sort.read(),
        direction: *completed_filters.direction.read(),
    };
    // Reads the same values without subscribing, for effects that must depend
    // only on their own trigger. The debounced search value is authoritative
    // here: a head refresh must repeat the request identity that produced the
    // loaded pages.
    let completed_request_snapshot = move || CompletedRequest {
        include_archived: *completed_filters.include_archived.peek(),
        search: completed_search.peek().clone(),
        status: *completed_filters.status.peek(),
        revision: *completed_filters.revision.peek(),
        latest_only: *completed_filters.latest_only.peek(),
        sort: *completed_filters.sort.peek(),
        direction: *completed_filters.direction.peek(),
    };

    let mut policy_on_build = use_signal(|| true);
    let mut policy_deployed_interval = use_signal(|| "24h".to_string());
    let mut policy_recent_interval = use_signal(|| "24h".to_string());
    let mut policy_archived_interval = use_signal(|| "168h".to_string());
    let mut policy_archived_enabled = use_signal(|| true);
    let mut policy_rebuild_to_scan = use_signal(|| false);
    let mut schedule_save_error = use_signal(|| Option::<String>::None);
    let schedule_saving = use_signal(|| false);

    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(LIVE_REFRESH_MS).await;
            live_refresh.set(live_refresh().wrapping_add(1));
        }
    });

    #[cfg(target_arch = "wasm32")]
    {
        let keydown_listener = use_hook(move || {
            let callback = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
                move |event: web_sys::KeyboardEvent| {
                    if event.key() == "Escape" {
                        if selected_scan.peek().is_some() {
                            close_scan_detail(selected_scan, detail_generation);
                        } else if schedule_open() {
                            schedule_open.set(false);
                        }
                    }
                },
            );
            if let Some(window) = web_sys::window() {
                let _ = window
                    .add_event_listener_with_callback("keydown", callback.as_ref().unchecked_ref());
            }
            Rc::new(callback)
        });
        let listener_for_drop = keydown_listener.clone();
        use_drop(move || {
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback(
                    "keydown",
                    listener_for_drop.as_ref().as_ref().unchecked_ref(),
                );
            }
        });
    }

    let mut stats = use_resource(move || {
        let _ = refresh();
        let _ = live_refresh();
        async { fetch_scanning_stats().await }
    });
    // Active is one bounded page. The live tick replaces it because no
    // accumulated pages exist for a collection without cursors.
    let active = use_resource(move || {
        let _ = refresh();
        let _ = live_refresh();
        async { fetch_scanning_scan_records(&active_records_query()).await }
    });
    // The head resource owns the Completed first page for the current request
    // identity. Filter changes, archive actions, and manual retries restart it,
    // which resets every accumulated page. It deliberately does not depend on
    // the live tick: a live refresh must not refetch accumulated pages.
    let completed_head = use_resource(move || {
        let _ = refresh();
        let request = completed_request();
        async move { fetch_scanning_scan_records(&request.to_query(None)).await }
    });
    // The archived badge needs a count the current page cannot report while
    // archived rows are included. The probe repeats the current filters with a
    // one-row page and archived rows excluded.
    let archived_probe = use_resource(move || {
        let _ = refresh();
        let _ = live_refresh();
        let request = completed_request();
        async move {
            if !request.include_archived {
                return None;
            }
            Some(fetch_scanning_scan_records(&request.archived_count_query()).await)
        }
    });
    let mut systems = use_resource(move || {
        let _ = refresh();
        async { fetch_scanning_systems(Some(SYSTEM_LIMIT)).await }
    });
    let environments = use_resource(|| async { fetch_environments().await });
    let mut schedule = use_resource(move || {
        let _ = schedule_refresh();
        async { fetch_scanning_schedule().await }
    });

    use_effect(move || {
        if !schedule_open() {
            return;
        }
        if let Some(Ok(policy)) = schedule.read().as_ref() {
            policy_on_build.set(policy.on_build);
            policy_deployed_interval.set(policy.deployed_interval.clone());
            policy_recent_interval.set(policy.recent_interval.clone());
            policy_archived_interval.set(policy.archived_interval.clone());
            policy_archived_enabled.set(policy.archived_enabled);
            policy_rebuild_to_scan.set(policy.rebuild_to_scan);
        }
    });

    // Selection is scoped to one collection's rows, so switching tabs clears
    // it. Filters persist per collection and are not reset here.
    use_effect(move || {
        let _ = tab();
        selected_rows.write().clear();
        completed_selection_anchor.set(None);
    });

    // Completed selection belongs to one exact server request identity. Clear
    // it when any filter or ordering value changes so hidden stale identities
    // cannot inflate the selected count or reappear under a later filter.
    use_effect(move || {
        let _ = completed_request();
        selected_rows.write().clear();
        completed_selection_anchor.set(None);
    });

    // A Completed anchor is meaningful only while that exact scan remains in
    // the loaded display order. Head refreshes can replace or update loaded
    // records without changing the request identity, so drop a stale anchor
    // when its row disappears. Existing selected IDs remain untouched until
    // the normal request/archive reset invalidates them.
    use_effect(move || {
        let anchor = completed_selection_anchor();
        if let Some(anchor) = anchor {
            let anchor_is_loaded = completed_pagination
                .read()
                .rows()
                .iter()
                .any(|row| row.scan_id == anchor);
            if !anchor_is_loaded {
                completed_selection_anchor.set(None);
            }
        }
    });

    // Debounce the server search so each keystroke does not restart Completed
    // pagination. The sequence guard drops superseded timers.
    use_effect(move || {
        let typed = completed_filters.query.read().trim().to_string();
        let sequence = *search_debounce.peek() + 1;
        search_debounce.set(sequence);
        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(SEARCH_DEBOUNCE_MS).await;
            if *search_debounce.peek() == sequence && *completed_search.peek() != typed {
                completed_search.set(typed);
            }
        });
    });

    // The head response is the authoritative first page for its request
    // identity. Every transition resets the accumulated pages, so an archive
    // action, filter change, or retry can never mix pages from two identities.
    use_effect(move || {
        let head = completed_head.read().clone();
        match head {
            Some(Ok(page)) => completed_pagination.write().reset(Some(page)),
            Some(Err(_)) | None => completed_pagination.write().reset(None),
        }
    });

    // Keep the last server-reported archived count so a reload never claims
    // zero archived rows while the replacement page is in flight.
    use_effect(move || {
        let reported = if *completed_filters.include_archived.read() {
            match &*archived_probe.read() {
                Some(Some(Ok(page))) => Some(page.hidden_archived.max(0)),
                _ => None,
            }
        } else {
            let state = completed_pagination.read();
            state.is_loaded().then(|| state.hidden_archived())
        };
        if let Some(count) = reported {
            archived_count.set(count);
        }
    });

    // CONCURRENCY: the live tick refreshes only the Completed head page. Later
    // pages keep the cursor bound captured by the first request, so no
    // accumulated page is refetched and no loaded row is duplicated. A failed
    // head refresh keeps the loaded rows and waits for the next tick.
    use_effect(move || {
        if live_refresh() == 0 {
            return;
        }
        let request = completed_request_snapshot();
        let Some(token) = completed_pagination.write().begin_head_refresh() else {
            return;
        };
        spawn(async move {
            let query = request.to_query(None);
            match fetch_scanning_scan_records_with_timeout(&query, HEAD_REFRESH_TIMEOUT_MS).await {
                Ok(page) => {
                    completed_pagination.write().complete_head_refresh(
                        &token,
                        page,
                        request.head_refresh_mode(),
                    );
                }
                Err(_) => {
                    completed_pagination.write().fail_head_refresh(&token);
                }
            }
        });
    });

    use_effect(move || {
        let _ = refresh();
        system_histories.write().clear();
        system_errors.write().clear();
        if tab() == ScanTab::Systems
            && let Some(system_id) = expanded_system()
        {
            let archived = system_archive_visibility(&system_archive_states.peek(), system_id);
            reload_system_history(
                system_id,
                system_histories,
                system_errors,
                loading_system,
                archived,
            );
        }
    });

    let active_value = resource_value(&active);
    let completed_value = completed_request();
    let completed_loading = completed_head.read().is_none();
    let completed_error = resource_error(&completed_head);
    // The infinite-scroll hook must run on every render, so it is created here
    // and passed to the panel. The reset key combines the request identity with
    // the pagination generation, so a filter change, an archive action, or any
    // other reload returns scroll paging to the first page. A live head refresh
    // keeps the generation and therefore keeps the loaded depth.
    let completed_scroll = use_infinite_scroll(
        format!(
            "{}|{}",
            completed_value.reset_key(),
            completed_pagination.read().generation()
        ),
        usize::from(COMPLETED_PAGE_LIMIT),
    );

    // The hook grows its requested count by one page whenever the sentinel
    // enters the scroll container. The strict comparison keeps the first render
    // from requesting a continuation before any scrolling happens, and keeps
    // one intersection from requesting more than one page.
    use_effect(move || {
        let requested = completed_scroll.count();
        let (loaded, wants_more) = {
            let state = completed_pagination.read();
            (
                state.rows().len(),
                state.has_more()
                    && !state.continuation_loading
                    && state.continuation_error.is_none(),
            )
        };
        if wants_more && requested > loaded {
            request_completed_continuation(
                completed_pagination,
                completed_head,
                completed_request_snapshot(),
            );
        }
        completed_scroll.recheck(requested.min(loaded));
    });

    let systems_value = systems
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let schedule_value: Option<ScanSchedulePolicyResponse> = schedule
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned();
    let env_colors = environments
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .map(|items| {
            items
                .iter()
                .map(|environment| {
                    (
                        environment.name.to_ascii_lowercase(),
                        environment.color_hex.clone(),
                    )
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let schedule_for_button = schedule_value.clone();

    rsx! {
        div { class: "scanning-view",
            div { class: "page-head scanning-head",
                div {
                    div { class: "scanning-title-line",
                        h1 { class: "page-title", "Scanning" }
                        span { class: "scanning-live", title: "Active scans and fleet totals refresh every 15 seconds", span { class: "scan-pulse" } "Live" }
                    }
                    p { class: "page-subtitle", "Exact CVE scan lifecycles and bounded vulnix diagnostics" }
                }
                div { class: "scanning-head-actions",
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| {
                            if let Some(policy) = schedule_for_button.clone() {
                                policy_on_build.set(policy.on_build);
                                policy_deployed_interval.set(policy.deployed_interval);
                                policy_recent_interval.set(policy.recent_interval);
                                policy_archived_interval.set(policy.archived_interval);
                                policy_archived_enabled.set(policy.archived_enabled);
                                policy_rebuild_to_scan.set(policy.rebuild_to_scan);
                            }
                            schedule_save_error.set(None);
                            schedule_open.set(true);
                        },
                        Icon { name: IconName::Gear, size: 14 }
                        " Schedule"
                    }
                }
            }

            if let Some(feedback) = action_feedback() {
                div { role: if feedback.success { "status" } else { "alert" }, class: if feedback.success { "sd-callout sd-callout-success scanning-alert" } else { "sd-callout sd-callout-danger scanning-alert" },
                    div { "{feedback.message}" }
                    button { class: "btn btn-ghost xs focus-ring", onclick: move |_| action_feedback.set(None), "Dismiss" }
                }
            }

            if let Some(Err(error)) = stats.read().as_ref() {
                div { role: "alert", class: "sd-callout sd-callout-danger scanning-alert",
                    div { "Scan summary could not be loaded: {error}" }
                    button { class: "btn btn-ghost xs focus-ring", onclick: move |_| stats.restart(), "Retry" }
                }
            }

            div { class: "stat-strip scanning-stats",
                if let Some(Ok(summary)) = stats.read().as_ref() {
                    { stat_card("Scanning now", &summary.scanning.to_string(), Some(&format!("{} queued · {} awaiting build · {} awaiting closure", summary.queued, summary.awaiting_build, summary.awaiting_closure)), "#60a5fa") }
                    { stat_card("Stale", &summary.stale.to_string(), Some("past rescan interval"), "#fbbf24") }
                    { stat_card("Never scanned", &summary.never_scanned.to_string(), None, "#9ca3af") }
                    if summary.failed > 0 {
                        button {
                            class: "stat scanning-stat-button focus-ring",
                            aria_label: "Open the newest failed scan",
                            aria_busy: failed_lookup_pending(),
                            onclick: move |_| {
                                tab.set(ScanTab::Completed);
                                open_newest_failed_scan(failed_lookup_pending, action_feedback, selected_scan, detail_state, detail_generation);
                            },
                            span { class: "stat-accent", style: "--stat-color:#f87171;" }
                            div { class: "stat-label", "Failed" }
                            div { class: "stat-value", style: "color:#f87171;", "{summary.failed}" }
                            div { class: "stat-meta", "Open newest failure" }
                        }
                    } else {
                        { stat_card("Failed", "0", None, "#34d399") }
                    }
                    { stat_card("Coverage", &format!("{}%", summary.coverage_percent), Some("configs with results"), "#34d399") }
                } else {
                    for (label, color) in [("Scanning now", "#60a5fa"), ("Stale", "#fbbf24"), ("Never scanned", "#9ca3af"), ("Failed", "#f87171"), ("Coverage", "#34d399")] {
                        { stat_card(label, "—", None, color) }
                    }
                }
            }

            section { class: "card scanning-card", aria_label: "CVE scans",
                div { class: "sd-tabs scanning-tabs", role: "tablist", aria_label: "Scan views",
                    onkeydown: move |event| {
                        let next = match event.key() {
                            Key::ArrowRight => match tab() { ScanTab::Active => ScanTab::Completed, ScanTab::Completed => ScanTab::Systems, ScanTab::Systems => ScanTab::Active },
                            Key::ArrowLeft => match tab() { ScanTab::Active => ScanTab::Systems, ScanTab::Completed => ScanTab::Active, ScanTab::Systems => ScanTab::Completed },
                            Key::Home => ScanTab::Active,
                            Key::End => ScanTab::Systems,
                            _ => return,
                        };
                        event.prevent_default();
                        tab.set(next);
                        focus_element_by_id(scan_tab_id(next));
                    },
                    { scan_tab_button(tab, ScanTab::Active, "Active", active_value.total, "scan-active-panel") }
                    { scan_tab_button(tab, ScanTab::Completed, "Completed", completed_pagination.read().visible_total(), "scan-completed-panel") }
                    { scan_tab_button(tab, ScanTab::Systems, "By system", systems_value.len() as i64, "scan-systems-panel") }
                }
                match tab() {
                    ScanTab::Active => rsx! {
                        div { id: "scan-active-panel", role: "tabpanel", aria_labelledby: "scan-active-tab",
                            { active_panel(
                                active_value.clone(),
                                active.read().is_none(),
                                resource_error(&active),
                                active_filters,
                                rows_context,
                                active,
                            ) }
                        }
                    },
                    ScanTab::Completed => rsx! {
                        div { id: "scan-completed-panel", role: "tabpanel", aria_labelledby: "scan-completed-tab",
                            { completed_panel(
                                completed_pagination,
                                completed_head,
                                completed_value.clone(),
                                completed_loading,
                                completed_error.clone(),
                                archived_count(),
                                completed_filters,
                                selected_rows,
                                completed_selection_anchor,
                                archive_pending,
                                rows_context,
                                completed_scroll,
                            ) }
                        }
                    },
                    ScanTab::Systems => rsx! {
                        div { id: "scan-systems-panel", role: "tabpanel", aria_labelledby: "scan-systems-tab",
                            { systems_panel(
                                systems_value.clone(),
                                systems.read().is_none(),
                                systems.read().as_ref().and_then(|result| result.as_ref().err()).map(ToString::to_string),
                                env_colors.clone(),
                                system_query,
                                system_environment,
                                expanded_system,
                                system_histories,
                                system_errors,
                                loading_system,
                                system_archive_states,
                                rows_context,
                                move || systems.restart(),
                            ) }
                        }
                    },
                }
            }

            if schedule_open() {
                { schedule_modal(
                    schedule_value.clone(),
                    schedule.read().as_ref().and_then(|result| result.as_ref().err()).map(ToString::to_string),
                    schedule_open,
                    policy_on_build,
                    policy_deployed_interval,
                    policy_recent_interval,
                    policy_archived_interval,
                    policy_archived_enabled,
                    policy_rebuild_to_scan,
                    schedule_save_error,
                    schedule_saving,
                    schedule_refresh,
                    selected_scan,
                    move || schedule.restart(),
                ) }
            }
            if let Some(selection) = selected_scan() {
                ScanDetailDrawer {
                    selection,
                    selected: selected_scan,
                    state: detail_state,
                    generation: detail_generation,
                    retry_pending: exact_retry_pending,
                    feedback: action_feedback,
                    refresh,
                }
            }
        }
    }
}

fn resource_value(
    resource: &Resource<Result<ScanningScanRecordsResponse, ApiClientError>>,
) -> ScanningScanRecordsResponse {
    resource
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned()
        .unwrap_or(ScanningScanRecordsResponse {
            items: Vec::new(),
            total: 0,
            hidden_archived: 0,
            has_more: false,
            next_cursor: None,
        })
}

fn resource_error(
    resource: &Resource<Result<ScanningScanRecordsResponse, ApiClientError>>,
) -> Option<String> {
    resource
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .map(ToString::to_string)
}

fn scan_tab_button(
    mut tab: Signal<ScanTab>,
    value: ScanTab,
    label: &'static str,
    count: i64,
    controls: &'static str,
) -> Element {
    let selected = tab() == value;
    rsx! {
        button {
            id: scan_tab_id(value),
            class: if selected { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
            role: "tab",
            aria_selected: selected,
            aria_controls: controls,
            tabindex: if selected { "0" } else { "-1" },
            onclick: move |_| tab.set(value),
            "{label}"
            span { class: "sd-tab-badge", "{count}" }
        }
    }
}

fn scan_tab_id(tab: ScanTab) -> &'static str {
    match tab {
        ScanTab::Active => "scan-active-tab",
        ScanTab::Completed => "scan-completed-tab",
        ScanTab::Systems => "scan-systems-tab",
    }
}

fn focus_element_by_id(id: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(id))
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = element.focus();
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = id;
}

/// Lists the fixed Active status filter values.
///
/// The values come from the server's nonterminal lifecycle states, not from
/// loaded rows, so the filter offers the same options on every load.
const ACTIVE_STATUS_OPTIONS: [&str; 4] = [
    "in_progress",
    "pending",
    "awaiting_build",
    "awaiting_closure",
];

/// Renders the server-filtered, server-ordered, keyset-paged Completed
/// collection.
///
/// Search, status, revision, latest-revision, ordering, and archive visibility
/// are request parameters, so every count and the row order describe the whole
/// matching collection instead of the loaded rows. Continuation pages are
/// appended in server order. A continuation failure keeps the loaded rows and
/// offers a retry rather than discarding evidence the operator can see.
#[allow(clippy::too_many_arguments)]
fn completed_panel(
    pagination: Signal<ScanRecordPaginationState>,
    head: Resource<Result<ScanningScanRecordsResponse, ApiClientError>>,
    request: CompletedRequest,
    loading: bool,
    error: Option<String>,
    archived_count: i64,
    filters: CompletedFilterSignals,
    mut selected_rows: Signal<HashSet<Uuid>>,
    mut selection_anchor: Signal<Option<Uuid>>,
    archive_pending: Signal<bool>,
    rows: RecordRowContext,
    scroll: InfiniteScroll,
) -> Element {
    let mut include_archived = filters.include_archived;
    let mut query = filters.query;
    let mut status = filters.status;
    let mut revision = filters.revision;
    let mut latest_only = filters.latest_only;
    let mut head = head;
    let state = pagination.read().clone();
    let records = state.rows().to_vec();
    let ordered_ids = Rc::new(
        records
            .iter()
            .map(|record| record.scan_id)
            .collect::<Vec<_>>(),
    );
    let loaded = records.len();
    let available = state.visible_total();
    let hidden_archived = state.hidden_archived();
    let has_more = state.has_more();
    let continuation_loading = state.continuation_loading;
    let continuation_error = state.continuation_error.clone();
    let narrowed = request.has_narrowing_filter();
    let selected = selected_rows();
    let archive_ids = records
        .iter()
        .filter(|row| selected.contains(&row.scan_id) && row.archived_at.is_none())
        .map(|row| row.scan_id)
        .collect::<Vec<_>>();
    let restore_ids = records
        .iter()
        .filter(|row| selected.contains(&row.scan_id) && row.archived_at.is_some())
        .map(|row| row.scan_id)
        .collect::<Vec<_>>();
    let archive_over_batch = archive_ids.len() > ARCHIVE_BATCH_MAX;
    let restore_over_batch = restore_ids.len() > ARCHIVE_BATCH_MAX;
    let request_for_button = request.clone();
    let request_for_error = request.clone();

    rsx! {
        div { class: "scan-toolbar",
            div { class: "q-search scanning-search",
                Icon { name: IconName::Search, size: 13 }
                input { class: "q-search-input", aria_label: "Search scans by configuration, flake, revision, scan ID, or derivation ID", placeholder: "Search scans…", value: query(), oninput: move |event| query.set(event.value()) }
                if !query().is_empty() { button { class: "btn-icon xs focus-ring", aria_label: "Clear scan search", onclick: move |_| query.set(String::new()), Icon { name: IconName::X, size: 13 } } }
            }
            select { class: "input filter-select focus-ring", aria_label: "Filter by scan status", value: status().as_param(), oninput: move |event| status.set(completed_status_from_value(&event.value())),
                option { value: "all", "All statuses" }
                option { value: "completed", "{status_meta(\"completed\").label}" }
                option { value: "failed", "{status_meta(\"failed\").label}" }
            }
            select { class: "input filter-select focus-ring", aria_label: "Filter by revision freshness", value: revision().as_param(), oninput: move |event| revision.set(completed_revision_from_value(&event.value())),
                option { value: "all", "All revisions" }
                option { value: "deployed", "Deployed" }
                option { value: "recent", "Latest per flake" }
                option { value: "superseded", "Superseded" }
            }
            button { class: if latest_only() { "btn btn-ghost xs focus-ring active-filter" } else { "btn btn-ghost xs focus-ring" }, aria_pressed: latest_only(), onclick: move |_| latest_only.toggle(), Icon { name: IconName::Star, size: 12 } " Latest per flake" }
            span { class: "filter-count", "{loaded} loaded · {available} matching" if has_more { " · more pages available" } }
            button {
                class: if include_archived() { "btn btn-ghost xs focus-ring active-filter scanning-archived-filter" } else { "btn btn-ghost xs focus-ring scanning-archived-filter" },
                aria_pressed: include_archived(),
                disabled: archived_count == 0 && !include_archived(),
                title: if include_archived() { "Hide archived scans" } else if archived_count == 0 { "No archived scans match this view" } else { "Show archived scans hidden by this view" },
                onclick: move |_| {
                    include_archived.toggle();
                    selected_rows.write().clear();
                    selection_anchor.set(None);
                },
                Icon { name: IconName::Archive, size: 12 }
                " Archived [{archived_count}]"
            }
        }

        div { class: "scanning-history-actions",
            span { "{selected.len()} selected" }
            span { class: "ms-hint", title: "Ctrl/Cmd-click toggles one scan · Shift-click selects the inclusive loaded range", "⌘/⇧-click to select" }
            button {
                class: "btn btn-ghost xs focus-ring",
                disabled: archive_ids.is_empty() || archive_pending() || archive_over_batch,
                title: if archive_over_batch { "Select 100 or fewer scans; one archive request carries at most 100 exact scan identities" } else { "Archive the selected exact scans" },
                onclick: move |_| apply_archive(archive_ids.clone(), true, selected_rows, selection_anchor, archive_pending, rows.feedback, rows.refresh),
                "Archive selected"
            }
            button {
                class: "btn btn-ghost xs focus-ring",
                disabled: restore_ids.is_empty() || archive_pending() || restore_over_batch,
                title: if restore_over_batch { "Select 100 or fewer scans; one restore request carries at most 100 exact scan identities" } else { "Restore the selected exact scans" },
                onclick: move |_| apply_archive(restore_ids.clone(), false, selected_rows, selection_anchor, archive_pending, rows.feedback, rows.refresh),
                "Restore selected"
            }
            if hidden_archived > 0 { span { class: "scanning-hidden-count", "{hidden_archived} archived scans hidden by retention view" } }
        }

        if let Some(error) = error {
            { load_error_state("Scans could not be loaded", &error, move || head.restart()) }
        } else if loading {
            div { class: "q-empty", role: "status", "Loading exact scan lifecycles…" }
        } else if records.is_empty() {
            div { class: "q-empty",
                if hidden_archived > 0 && !include_archived() {
                    h3 { "Completed scans are hidden" }
                    p { "The current retention view hides {hidden_archived} archived scan(s). Include archived scans to review or restore them." }
                } else if narrowed {
                    h3 { "No scans match these filters" }
                    p { "The server found no terminal scan matching these filters." }
                    button { class: "btn btn-ghost xs focus-ring", onclick: move |_| reset_completed_filters(query, status, revision, latest_only), "Reset filters" }
                } else {
                    h3 { "No completed scan history" }
                    p { "Terminal scan lifecycles will remain here as history." }
                }
            }
        } else {
            div { class: "scanning-table-wrap",
                table { class: "sys-table scanning-table",
                    thead { tr {
                        th { span { class: "sr-only", "Select" } }
                        { completed_sort_header("Configuration", ScanRecordSortParam::Configuration, filters) }
                        { completed_sort_header("Revision", ScanRecordSortParam::Revision, filters) }
                        { completed_sort_header("Status", ScanRecordSortParam::Status, filters) }
                        { completed_sort_header("Findings", ScanRecordSortParam::Severity, filters) }
                        { completed_sort_header("Last scan", ScanRecordSortParam::Timestamp, filters) }
                        th { "Trigger" }
                        th { class: "scanning-actions-heading", span { class: "sr-only", "Actions" } }
                    } }
                    tbody { for row in records {
                        { record_row(
                            row,
                            Some(selected_rows),
                            Some(CompletedRowSelection {
                                anchor: selection_anchor,
                                ordered_ids: ordered_ids.clone(),
                            }),
                            rows,
                        ) }
                    } }
                }
            }
            if let Some(message) = continuation_error {
                div { role: "alert", class: "sd-callout sd-callout-danger scanning-inline-alert", "data-testid": "scanning-completed-continuation-error",
                    div { "More completed scans could not be loaded: {message}" }
                    button {
                        class: "btn btn-ghost xs focus-ring",
                        disabled: continuation_loading,
                        aria_label: "Retry loading more completed scans",
                        onclick: move |_| request_completed_continuation(pagination, head, request_for_error.clone()),
                        "Retry"
                    }
                }
            }
            div { class: "scanning-pagination",
                span { "{loaded} of {available} loaded" }
                if has_more {
                    button {
                        class: "btn btn-ghost xs focus-ring",
                        "data-testid": "scanning-completed-load-more",
                        disabled: continuation_loading,
                        aria_busy: continuation_loading,
                        aria_label: "Load more completed scans",
                        onclick: move |_| request_completed_continuation(pagination, head, request_for_button.clone()),
                        if continuation_loading { "Loading more…" } else { "Load more" }
                    }
                } else {
                    span { class: "scanning-end-of-history", role: "status", "End of completed history" }
                }
            }
            if has_more && !continuation_loading {
                // The sentinel drives scroll-triggered continuation. The Load
                // more button above carries the accessible affordance, so the
                // sentinel stays out of the accessibility tree.
                div {
                    class: "infinite-sentinel scanning-sentinel",
                    "data-sentinel": scroll.sentinel_id(),
                    aria_hidden: "true",
                    onmounted: move |_| scroll.check_and_register(),
                }
            }
        }
    }
}

/// Renders the single bounded Active page.
///
/// The server does not filter or order nonterminal collections by request, so
/// this panel filters the loaded page in the browser. When the server reports
/// more matching rows than the page holds, the panel discloses the bound and
/// withholds column sorting instead of ordering an incomplete collection.
fn active_panel(
    response: ScanningScanRecordsResponse,
    loading: bool,
    error: Option<String>,
    filters: ActiveFilterSignals,
    rows: RecordRowContext,
    source: Resource<Result<ScanningScanRecordsResponse, ApiClientError>>,
) -> Element {
    let mut source = source;
    let mut query = filters.query;
    let mut status = filters.status;
    let mut revision = filters.revision;
    let mut latest_only = filters.latest_only;
    let sort = filters.sort;
    let descending = filters.descending;
    let visible = filter_and_sort_active_records(
        &response.items,
        &query(),
        &status(),
        &revision(),
        latest_only(),
        sort(),
        descending(),
    );
    let filtered = visible.len();
    let loaded = response.items.len();
    let available = visible_record_total(&response);
    let capped = available > loaded as i64;

    rsx! {
        div { class: "scan-toolbar",
            div { class: "q-search scanning-search",
                Icon { name: IconName::Search, size: 13 }
                input { class: "q-search-input", aria_label: "Search scans by configuration, flake, revision, scan ID, or derivation ID", placeholder: "Search scans…", value: query(), oninput: move |event| query.set(event.value()) }
                if !query().is_empty() { button { class: "btn-icon xs focus-ring", aria_label: "Clear scan search", onclick: move |_| query.set(String::new()), Icon { name: IconName::X, size: 13 } } }
            }
            select { class: "input filter-select focus-ring", aria_label: "Filter by scan status", value: status(), oninput: move |event| status.set(event.value()),
                option { value: "all", "All statuses" }
                for key in ACTIVE_STATUS_OPTIONS {
                    option { value: key, "{status_meta(key).label}" }
                }
            }
            select { class: "input filter-select focus-ring", aria_label: "Filter by revision freshness", value: revision(), oninput: move |event| revision.set(event.value()),
                option { value: "all", "All revisions" }
                option { value: "deployed", "Deployed" }
                option { value: "recent", "Latest per flake" }
                option { value: "superseded", "Superseded" }
            }
            button { class: if latest_only() { "btn btn-ghost xs focus-ring active-filter" } else { "btn btn-ghost xs focus-ring" }, aria_pressed: latest_only(), onclick: move |_| latest_only.toggle(), Icon { name: IconName::Star, size: 12 } " Latest per flake" }
            span { class: "filter-count", "{filtered} visible · {loaded} loaded" if capped { " · the server reports {available} active scans; this page holds the first {loaded} and sorting is unavailable" } }
        }

        if let Some(error) = error {
            { load_error_state("Scans could not be loaded", &error, move || source.restart()) }
        } else if loading {
            div { class: "q-empty", role: "status", "Loading exact scan lifecycles…" }
        } else if visible.is_empty() {
            div { class: "q-empty",
                if response.items.is_empty() {
                    h3 { "No active scans" }
                    p { "Queued, scanning, and prerequisite wait states will appear here." }
                } else {
                    h3 { "No scans match these filters" }
                    p { if capped { "No loaded active scan matches these filters. The server reports more active scans than this page holds." } else { "The result count reflects the current filters." } }
                    button { class: "btn btn-ghost xs focus-ring", onclick: move |_| reset_active_filters(query, status, revision, latest_only), "Reset filters" }
                }
            }
        } else {
            div { class: "scanning-table-wrap",
                table { class: "sys-table scanning-table",
                    thead { tr {
                        { active_sort_header("Configuration", ScanSort::Configuration, filters, capped) }
                        { active_sort_header("Revision", ScanSort::Revision, filters, capped) }
                        { active_sort_header("Status", ScanSort::Status, filters, capped) }
                        { active_sort_header("Findings", ScanSort::Severity, filters, capped) }
                        { active_sort_header("Last scan", ScanSort::Timestamp, filters, capped) }
                        th { "Trigger" }
                        th { class: "scanning-actions-heading", span { class: "sr-only", "Actions" } }
                    } }
                    tbody { for row in visible { { record_row(row, None, None, rows) } } }
                }
            }
        }
    }
}

/// Maps a status select value to the validated server parameter.
///
/// An unknown value falls back to [`ScanRecordStatusParam::All`] so a stale
/// DOM value can never produce a request the server rejects.
fn completed_status_from_value(value: &str) -> ScanRecordStatusParam {
    match value {
        "completed" => ScanRecordStatusParam::Completed,
        "failed" => ScanRecordStatusParam::Failed,
        _ => ScanRecordStatusParam::All,
    }
}

/// Maps a revision select value to the validated server parameter.
///
/// An unknown value falls back to [`ScanRecordRevisionParam::All`] for the same
/// reason as [`completed_status_from_value`].
fn completed_revision_from_value(value: &str) -> ScanRecordRevisionParam {
    match value {
        "deployed" => ScanRecordRevisionParam::Deployed,
        "recent" => ScanRecordRevisionParam::Recent,
        "superseded" => ScanRecordRevisionParam::Superseded,
        _ => ScanRecordRevisionParam::All,
    }
}

/// Returns the direction a newly selected Completed sort key starts with.
///
/// Severity and timestamp read newest or worst first; names and revisions read
/// in ascending order.
fn default_sort_direction(key: ScanRecordSortParam) -> ScanRecordDirectionParam {
    match key {
        ScanRecordSortParam::Severity | ScanRecordSortParam::Timestamp => {
            ScanRecordDirectionParam::Desc
        }
        _ => ScanRecordDirectionParam::Asc,
    }
}

fn reset_active_filters(
    mut query: Signal<String>,
    mut status: Signal<String>,
    mut revision: Signal<String>,
    mut latest: Signal<bool>,
) {
    query.set(String::new());
    status.set("all".to_string());
    revision.set("all".to_string());
    latest.set(false);
}

/// Clears the Completed request filters without changing the ordering.
///
/// Clearing them restarts pagination because the server's cursor fingerprint
/// covers every filter value.
fn reset_completed_filters(
    mut query: Signal<String>,
    mut status: Signal<ScanRecordStatusParam>,
    mut revision: Signal<ScanRecordRevisionParam>,
    mut latest: Signal<bool>,
) {
    query.set(String::new());
    status.set(ScanRecordStatusParam::All);
    revision.set(ScanRecordRevisionParam::All);
    latest.set(false);
}

/// Renders one Completed column header that reorders the whole collection.
fn completed_sort_header(
    label: &'static str,
    key: ScanRecordSortParam,
    filters: CompletedFilterSignals,
) -> Element {
    let mut sort = filters.sort;
    let mut direction = filters.direction;
    let active = sort() == key;
    let descending = direction() == ScanRecordDirectionParam::Desc;
    rsx! {
        th { aria_sort: if !active { "none" } else if descending { "descending" } else { "ascending" },
            button {
                class: if active { "th-sort focus-ring on" } else { "th-sort focus-ring" },
                aria_label: format!("Sort by {label}"),
                onclick: move |_| {
                    if sort() == key {
                        direction.set(if direction() == ScanRecordDirectionParam::Desc { ScanRecordDirectionParam::Asc } else { ScanRecordDirectionParam::Desc });
                    } else {
                        sort.set(key);
                        direction.set(default_sort_direction(key));
                    }
                },
                "{label}" Icon { name: if active && descending { IconName::ChevronDown } else { IconName::ChevronUp }, size: 10 }
            }
        }
    }
}

/// Renders one Active column header.
///
/// The header stays a plain label when the loaded page is incomplete, because
/// ordering only the loaded rows would misrepresent the collection.
fn active_sort_header(
    label: &'static str,
    key: ScanSort,
    filters: ActiveFilterSignals,
    capped: bool,
) -> Element {
    let mut sort = filters.sort;
    let mut descending = filters.descending;
    if capped {
        return rsx! {
            th { aria_sort: "none", title: "Sorting needs the complete active collection", "{label}" }
        };
    }
    let active = sort() == key;
    rsx! {
        th { aria_sort: if !active { "none" } else if descending() { "descending" } else { "ascending" },
            button { class: if active { "th-sort focus-ring on" } else { "th-sort focus-ring" }, aria_label: format!("Sort by {label}"), onclick: move |_| { if sort() == key { descending.toggle(); } else { sort.set(key); descending.set(matches!(key, ScanSort::Severity | ScanSort::Timestamp)); } },
                "{label}" Icon { name: if active && descending() { IconName::ChevronDown } else { IconName::ChevronUp }, size: 10 }
            }
        }
    }
}

/// Renders one scan lifecycle row.
///
/// `selection` is `Some` only for collections that support archive and restore
/// actions. Active rows have no selection column because nonterminal scans are
/// neither archivable nor cancellable.
fn record_row(
    row: ScanningScanRecordResponse,
    selection_state: Option<Signal<HashSet<Uuid>>>,
    completed_selection: Option<CompletedRowSelection>,
    context: RecordRowContext,
) -> Element {
    let retry_pending = context.retry_pending;
    let feedback = context.feedback;
    let refresh = context.refresh;
    let selected_scan = context.selected_scan;
    let detail_state = context.detail_state;
    let detail_generation = context.detail_generation;
    let meta = status_meta(&row.status);
    let selected =
        selection_state.is_some_and(|selected_rows| selected_rows.read().contains(&row.scan_id));
    let has_completed_selection = completed_selection.is_some();
    let completed_selection_for_click = completed_selection.clone();
    let completed_selection_for_checkbox = completed_selection.clone();
    let row_class = format!(
        "scanning-record{}{}{}",
        if selection_state.is_some() {
            " selectable"
        } else {
            ""
        },
        if selected { " row-checked" } else { "" },
        if row.archived_at.is_some() {
            " archived"
        } else {
            ""
        },
    );
    let selection = ScanDetailSelection {
        scan_id: row.scan_id,
        label: format!("{} · {}", row.hostname, commit_label(&row.commit_hash)),
    };
    let prerequisite_build_failure = is_prerequisite_build_failure(&row);
    let can_retry = row.status == "failed" && !prerequisite_build_failure;
    let relation = revision_class(&row);
    let relation_label = match relation {
        "deployed" => "Deployed",
        "recent" => "Recent",
        _ => "Superseded",
    };
    let configuration_meta = match row.flake_name.as_deref() {
        Some(flake) => format!("{flake} · {}", commit_label(&row.commit_hash)),
        None => commit_label(&row.commit_hash),
    };
    rsx! {
        tr {
            key: "{row.scan_id}",
            "data-testid": "scanning-record-{row.scan_id}",
            class: row_class,
            onmousedown: move |event| {
                if has_completed_selection && event.modifiers().shift() {
                    event.prevent_default();
                    event.stop_propagation();
                }
            },
            onclick: move |event| {
                let shift = event.modifiers().shift();
                let toggle = event.modifiers().ctrl() || event.modifiers().meta();
                if let (Some(mut selected_rows), Some(mut selection)) = (selection_state, completed_selection_for_click.clone()) {
                    if shift || toggle {
                        event.prevent_default();
                        event.stop_propagation();
                        let gesture = if shift {
                            CompletedSelectionGesture::ShiftRange
                        } else {
                            CompletedSelectionGesture::ModifierToggle
                        };
                        let (next, next_anchor) = completed_range_selection(
                            &selected_rows.read(),
                            (selection.anchor)(),
                            row.scan_id,
                            selection.ordered_ids.as_slice(),
                            gesture,
                        );
                        selected_rows.set(next);
                        selection.anchor.set(next_anchor);
                    }
                }
            },
            if let Some(mut selected_rows) = selection_state {
                td {
                    input {
                        r#type: "checkbox",
                        aria_label: format!("Select scan {}", row.scan_id),
                        checked: selected,
                        onclick: move |event| event.stop_propagation(),
                        onchange: move |event| {
                            event.stop_propagation();
                            if let Some(mut selection) = completed_selection_for_checkbox.clone() {
                                let (next, next_anchor) = completed_range_selection(
                                    &selected_rows.read(),
                                    (selection.anchor)(),
                                    row.scan_id,
                                    selection.ordered_ids.as_slice(),
                                    CompletedSelectionGesture::Checkbox { checked: event.checked() },
                                );
                                selected_rows.set(next);
                                selection.anchor.set(next_anchor);
                            }
                        }
                    }
                }
            }
            td { div { class: "scanning-config-name", "{row.hostname}" } div { class: "scanning-history-flake", "{configuration_meta}" } }
            td { span { class: if relation == "deployed" { "chip chip-healthy" } else if relation == "recent" { "chip chip-info" } else { "chip chip-unknown" }, "{relation_label}" } }
            td {
                span { class: "chip {meta.class}", span { class: "chip-dot", style: "background:{meta.color};" } "{meta.label}" }
                if let Some(reason) = row.wait_reason.as_deref() { div { class: "scanning-wait", "Awaiting: {reason}" } }
                if let Some(failure) = row.failure.as_deref() { div { class: "scanning-row-failure", title: "{failure}", "{bounded_failure_preview(failure)}" } }
                if row.archived_at.is_some() { div { class: "scanning-archived-label", Icon { name: IconName::Archive, size: 9 } " Archived" } }
            }
            td { { findings(row.critical_count, row.high_count, row.medium_count, row.low_count, row.status == "completed") } }
            td { class: "scanning-last-scan", title: "{record_time(&row).to_rfc3339()}", "{relative_time(record_time(&row))}" }
            td { if let Some(trigger) = row.source_trigger.as_deref() { span { class: "chip chip-unknown scanning-trigger", "{trigger}" } } else { span { class: "scanning-unavailable", "Not recorded" } } }
            td { div { class: "row-actions scanning-row-actions",
                if can_retry { button { class: "btn btn-ghost xs focus-ring", disabled: retry_pending.read().contains(&row.derivation_id), onclick: { let label = format!("{} {}", row.hostname, commit_label(&row.commit_hash)); move |event| { event.stop_propagation(); retry_exact_scan(row.derivation_id, label.clone(), retry_pending, feedback, refresh); } }, Icon { name: IconName::Sync, size: 11 } " Retry exact" } }
                button { class: "btn-icon focus-ring", aria_label: format!("Open details for scan {}", row.scan_id), title: "Open exact scan details", onclick: move |event| { event.stop_propagation(); load_scan_detail(selection.clone(), selected_scan, detail_state, detail_generation); }, Icon { name: IconName::Terminal, size: 14 } }
            } }
        }
    }
}

/// Archives or restores the selected exact scan identities.
///
/// The request carries the exact scan IDs the operator selected. It never
/// derives identities from a filter or a page, so a concurrent reload cannot
/// widen the action.
///
/// On success the shared refresh counter advances. That restarts the Completed
/// head request and clears cached per-system history, which resets every
/// accumulated page for the affected scopes and reloads authoritative archive
/// counts. Archiving hides retained evidence; it never deletes a scan.
///
/// The server rejects a request with more than [`ARCHIVE_BATCH_MAX`] scan IDs,
/// so the calling panel disables the action before that bound is reached.
fn apply_archive(
    scan_ids: Vec<Uuid>,
    archived: bool,
    mut selected: Signal<HashSet<Uuid>>,
    mut selection_anchor: Signal<Option<Uuid>>,
    mut pending: Signal<bool>,
    mut feedback: Signal<Option<ScanActionFeedback>>,
    mut refresh: Signal<u64>,
) {
    if scan_ids.is_empty() || scan_ids.len() > ARCHIVE_BATCH_MAX || pending() {
        return;
    }
    pending.set(true);
    spawn(async move {
        match update_scanning_archive_state(scan_ids, archived).await {
            Ok(result) => {
                feedback.set(Some(ScanActionFeedback {
                    message: format!(
                        "{} {} of {} requested scan(s).",
                        if archived { "Archived" } else { "Restored" },
                        result.changed,
                        result.requested
                    ),
                    success: true,
                }));
                selected.write().clear();
                selection_anchor.set(None);
                refresh.set(refresh().wrapping_add(1));
            }
            Err(error) => feedback.set(Some(ScanActionFeedback {
                message: format!("Archive state could not be updated: {error}"),
                success: false,
            })),
        }
        pending.set(false);
    });
}

#[allow(clippy::too_many_arguments)]
fn systems_panel(
    rows: Vec<ScanningSystemsItemResponse>,
    loading: bool,
    error: Option<String>,
    env_colors: HashMap<String, String>,
    mut query: Signal<String>,
    mut environment: Signal<String>,
    expanded: Signal<Option<Uuid>>,
    histories: Signal<HashMap<(Uuid, bool), SystemHistoryData>>,
    errors: Signal<HashMap<(Uuid, bool), String>>,
    loading_system: Signal<Option<(Uuid, bool)>>,
    mut archive_states: Signal<HashMap<Uuid, bool>>,
    context: RecordRowContext,
    retry: impl FnMut() + 'static,
) -> Element {
    let mut retry = retry;
    let retry_pending = context.retry_pending;
    let feedback = context.feedback;
    let refresh = context.refresh;
    let search = query().trim().to_ascii_lowercase();
    let selected_environment = environment();
    let mut environment_names = rows
        .iter()
        .filter_map(|row| row.environment.clone())
        .collect::<Vec<_>>();
    environment_names.sort();
    environment_names.dedup();
    let mut visible = rows
        .iter()
        .filter(|row| {
            (search.is_empty() || row.hostname.to_ascii_lowercase().contains(&search))
                && (selected_environment == "all"
                    || row.environment.as_deref() == Some(selected_environment.as_str()))
        })
        .cloned()
        .collect::<Vec<_>>();
    visible.sort_by(|left, right| {
        left.hostname
            .to_ascii_lowercase()
            .cmp(&right.hostname.to_ascii_lowercase())
            .then_with(|| left.system_id.cmp(&right.system_id))
    });
    let visible_count = format!("{} visible · {} loaded", visible.len(), rows.len());

    rsx! {
        div { class: "scan-toolbar",
            div { class: "q-search scanning-search", Icon { name: IconName::Search, size: 13 } input { class: "q-search-input", aria_label: "Search systems", placeholder: "Search systems…", value: query(), oninput: move |event| query.set(event.value()) } }
            select { class: "input filter-select focus-ring", aria_label: "Filter systems by environment", value: environment(), oninput: move |event| environment.set(event.value()), option { value: "all", "All environments" } for value in environment_names { option { value: "{value}", "{value}" } } }
            span { class: "filter-count", "{visible_count}" }
            button { class: "btn btn-ghost xs focus-ring", disabled: query().is_empty() && environment() == "all", onclick: move |_| { query.set(String::new()); environment.set("all".to_string()); }, "Reset" }
        }
        if let Some(error) = error { { load_error_state("Systems could not be loaded", &error, move || retry()) } }
        else if loading { div { class: "q-empty", role: "status", "Loading system scan history…" } }
        else if visible.is_empty() { div { class: "q-empty", h3 { if rows.is_empty() { "No system revision history" } else { "No systems match these filters" } } } }
        else { div { class: "scanning-table-wrap",
            table { class: "sys-table scanning-table scanning-systems-table",
                thead { tr { th { "System" } th { "Environment" } th { "Revision coverage" } th { "Current findings" } th { span { class: "sr-only", "Actions" } } } }
                tbody { for system in visible {
                    {
                        let system_id = system.system_id;
                        let open = expanded() == Some(system_id);
                        let archive_state = system_archive_visibility(&archive_states.read(), system_id);
                        let history = histories.read().get(&(system_id, archive_state)).cloned();
                        let history_error = errors.read().get(&(system_id, archive_state)).cloned();
                        rsx! {
                            tr { key: "system-{system_id}", class: if open { "scanning-system-row expanded" } else { "scanning-system-row" },
                                td { button { class: "scanning-system-toggle focus-ring", aria_expanded: open, onclick: move |_| toggle_system_history(system_id, expanded), Icon { name: if open { IconName::ChevronDown } else { IconName::ChevronRight }, size: 12 } span { class: "scanning-config-name", "{system.hostname}" } } }
                                td { if let Some(name) = system.environment.clone() { if let Some(color) = env_colors.get(&name.to_ascii_lowercase()) { EnvBadge { name, fg: color.clone(), bg: format!("color-mix(in oklab, {color} 14%, var(--cf-card-bg))"), border: color.clone() } } else { EnvBadge { name } } } else { span { class: "scanning-unavailable", "Unassigned" } } }
                                td { div { class: "scanning-system-counts", span { "{system.scanned} scanned" } if system.stale > 0 { span { class: "stale", "{system.stale} stale" } } if system.needs_build > 0 { span { class: "needs", "{system.needs_build} needs build" } } if system.unscanned > 0 { span { "{system.unscanned} never scanned" } } } }
                                td { { findings(system.current_crit as i32, system.current_high as i32, 0, 0, true) } }
                                td { if let Some(derivation_id) = system.current_derivation_id { button { class: "btn btn-ghost xs focus-ring", disabled: retry_pending.read().contains(&derivation_id), title: "Check the exact currently deployed derivation now", onclick: { let label = format!("{} deployed revision", system.hostname); move |_| retry_exact_scan(derivation_id, label.clone(), retry_pending, feedback, refresh) }, Icon { name: IconName::Sync, size: 11 } " Check now" } } }
                            }
                            if open { tr { class: "scan-sys-expand-row", td { colspan: 5,
                                div { class: "scan-sys-expand",
                                    div { class: "scan-sys-expand-head",
                                        span { "Exact revision history · newest first" }
                                        label { class: "scanning-include-archived", input { r#type: "checkbox", checked: archive_state, onchange: move |event| {
                                            let include_archived = event.checked();
                                            set_system_archive_visibility(&mut archive_states.write(), system_id, include_archived);
                                            reload_system_history(system_id, histories, errors, loading_system, include_archived);
                                        } } " Include archived" }
                                    }
                                    if let Some(error) = history_error { { load_error_state("Revision history could not be loaded", &error, move || reload_system_history(system_id, histories, errors, loading_system, archive_state)) } }
                                    else if loading_system() == Some((system_id, archive_state)) { div { class: "q-empty scanning-system-state", role: "status", "Loading exact history…" } }
                                    else if let Some(history) = history {
                                        if history.scans.items.is_empty() && history.derivations.iter().all(|row| row.scan_id.is_some()) { div { class: "q-empty scanning-system-state", if history.scans.hidden_archived > 0 { "{history.scans.hidden_archived} archived scan(s) are hidden by the retention view." } else { "No exact scans or unscanned revisions are recorded for this system." } } }
                                        else { { system_history_table(&system, history, context) } }
                                    }
                                }
                            } } }
                        }
                    }
                } }
            }
        } }
    }
}

fn toggle_system_history(system_id: Uuid, mut expanded: Signal<Option<Uuid>>) {
    if expanded() == Some(system_id) {
        expanded.set(None);
    } else {
        expanded.set(Some(system_id));
    }
}

/// Loads one system's bounded exact revision history.
///
/// The server rejects continuation cursors for the history collection, so this
/// request is one explicitly bounded page. [`system_history_table`] discloses
/// the bound whenever the server reports more retained scans than the page
/// holds.
fn reload_system_history(
    system_id: Uuid,
    mut histories: Signal<HashMap<(Uuid, bool), SystemHistoryData>>,
    mut errors: Signal<HashMap<(Uuid, bool), String>>,
    mut loading: Signal<Option<(Uuid, bool)>>,
    include_archived: bool,
) {
    let key = (system_id, include_archived);
    loading.set(Some(key));
    errors.write().remove(&key);
    spawn(async move {
        let scans =
            fetch_scanning_scan_records(&system_history_query(system_id, include_archived)).await;
        let derivations =
            fetch_scanning_system_scans(&system_id, Some(SYSTEM_DERIVATION_LIMIT)).await;
        match (scans, derivations) {
            (Ok(scans), Ok(derivations)) => {
                histories
                    .write()
                    .insert(key, SystemHistoryData { scans, derivations });
            }
            (Err(error), _) | (_, Err(error)) => {
                errors.write().insert(key, error.to_string());
            }
        }
        if loading() == Some(key) {
            loading.set(None);
        }
    });
}

fn system_history_table(
    system: &ScanningSystemsItemResponse,
    history: SystemHistoryData,
    context: RecordRowContext,
) -> Element {
    let retry_pending = context.retry_pending;
    let feedback = context.feedback;
    let refresh = context.refresh;
    let selected_scan = context.selected_scan;
    let detail_state = context.detail_state;
    let detail_generation = context.detail_generation;
    let hidden_archived = history.scans.hidden_archived;
    let loaded_scans = history.scans.items.len();
    let available_scans = visible_record_total(&history.scans);
    let bounded = available_scans > loaded_scans as i64;
    let revision_relations = history
        .derivations
        .iter()
        .map(|row| (row.derivation_id, (row.is_current, row.is_latest_per_flake)))
        .collect::<HashMap<_, _>>();
    let rows = system_history_entries(history);
    rsx! {
        if hidden_archived > 0 { div { class: "scanning-hidden-count", "{hidden_archived} archived scan(s) hidden" } }
        if bounded { div { class: "scanning-hidden-count", "Showing the newest {loaded_scans} of {available_scans} retained scans for this system" } }
        div { class: "scan-sys-expand-table-wrap", table { class: "scanning-history-table",
            thead { tr { th { "Revision" } th { "Relation" } th { "Status" } th { "Findings" } th { "Timestamp" } th { span { class: "sr-only", "Actions" } } } }
            tbody { for entry in rows {
                match entry {
                SystemHistoryEntry::Scan(row) => {
                    let relation = match (row.is_current, row.is_latest_per_flake) {
                        (true, _) => "Deployed",
                        (false, true) => "Recent",
                        _ if system.current_derivation_id == Some(row.derivation_id) => "Deployed",
                        _ => match revision_relations.get(&row.derivation_id).copied() {
                            Some((true, _)) => "Deployed",
                            Some((false, true)) => "Recent",
                            _ => "Superseded config",
                        },
                    };
                    let meta = status_meta(&row.status);
                    let selection = ScanDetailSelection { scan_id: row.scan_id, label: format!("{} · {}", row.hostname, commit_label(&row.commit_hash)) };
                    let revision = row.commit_hash.as_deref().unwrap_or("Revision unavailable");
                    rsx! { tr { key: "history-{row.scan_id}", class: if row.archived_at.is_some() { "scanning-record archived" } else { "scanning-record" },
                        td { div { class: "scanning-full-revision mono", "{revision}" } }
                        td { span { class: if relation == "Deployed" { "chip chip-healthy" } else { "chip chip-unknown" }, "{relation}" } }
                        td { span { class: "chip {meta.class}", "{meta.label}" } if let Some(reason) = row.wait_reason.as_deref() { div { class: "scanning-wait", "Awaiting: {reason}" } } if let Some(failure) = row.failure.as_deref() { div { class: "scanning-row-failure", title: "{failure}", "{bounded_failure_preview(failure)}" } } if row.archived_at.is_some() { div { class: "scanning-archived-label", Icon { name: IconName::Archive, size: 9 } " Archived" } } }
                        td { { findings(row.critical_count, row.high_count, row.medium_count, row.low_count, row.status == "completed") } }
                        td { class: "scanning-last-scan", title: "{record_time(&row).to_rfc3339()}", "{relative_time(record_time(&row))}" }
                        td { div { class: "row-actions scanning-row-actions",
                            if row.status == "failed" && !is_prerequisite_build_failure(&row) { button { class: "btn btn-ghost xs focus-ring", disabled: retry_pending.read().contains(&row.derivation_id), onclick: { let label = format!("{} {}", row.hostname, commit_label(&row.commit_hash)); move |_| retry_exact_scan(row.derivation_id, label.clone(), retry_pending, feedback, refresh) }, "Retry exact" } }
                            button { class: "btn-icon focus-ring", aria_label: format!("Open details for scan {}", row.scan_id), onclick: move |_| load_scan_detail(selection.clone(), selected_scan, detail_state, detail_generation), Icon { name: IconName::Terminal, size: 13 } }
                        } }
                    } }
                },
                SystemHistoryEntry::NoScan(row) => {
                    let relation = if row.is_current { "Deployed" } else if row.is_latest_per_flake { "Recent" } else { "Superseded config" };
                    let status = if row.rescan_eligible { "never_scanned" } else { "needs_build" };
                    let meta = status_meta(status);
                    let revision = row.commit_hash.as_deref().unwrap_or("Revision unavailable");
                    rsx! { tr { key: "unscanned-{row.derivation_id}", class: "scanning-record",
                        td { div { class: "scanning-full-revision mono", "{revision}" } div { class: "scanning-record-id mono", "drv {row.derivation_id} · no scan" } }
                        td { span { class: if relation == "Deployed" { "chip chip-healthy" } else { "chip chip-unknown" }, "{relation}" } }
                        td { span { class: "chip {meta.class}", "{meta.label}" } }
                        td { span { class: "scanning-unavailable", "Not available" } }
                        td { class: "scanning-last-scan", "Never" }
                        td { div { class: "row-actions scanning-row-actions",
                            if row.rescan_eligible { button { class: "btn btn-ghost xs focus-ring", disabled: retry_pending.read().contains(&row.derivation_id), onclick: { let label = format!("{} {}", row.hostname, commit_label(&row.commit_hash)); move |_| retry_exact_scan(row.derivation_id, label.clone(), retry_pending, feedback, refresh) }, "Check now" } }
                        } }
                    } }
                },
                }
            } }
        } }
    }
}

#[component]
fn ScanDetailDrawer(
    selection: ScanDetailSelection,
    mut selected: Signal<Option<ScanDetailSelection>>,
    mut state: Signal<ScanDetailState>,
    generation: Signal<u64>,
    retry_pending: Signal<HashSet<i32>>,
    feedback: Signal<Option<ScanActionFeedback>>,
    refresh: Signal<u64>,
) -> Element {
    let mut search = use_signal(String::new);
    let mut match_position = use_signal(|| 0_usize);
    let mut now = use_signal(Utc::now);
    let mut tab = use_signal(|| ScanDetailTab::Log);
    let selection_id = selection.scan_id;
    let poll_selection = selection.clone();

    use_effect(move || {
        let scan_id = selection_id;
        search.set(String::new());
        match_position.set(0);
        tab.set(ScanDetailTab::Log);
        let _ = scan_id;
    });
    use_effect(move || {
        let running = matches!(&*state.read(), ScanDetailState::Loaded(detail) if detail.status == "in_progress");
        if running {
            let refresh_selection = poll_selection.clone();
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(DETAIL_POLL_MS).await;
                if selected.peek().as_ref().map(|item| item.scan_id)
                    == Some(refresh_selection.scan_id)
                    && matches!(&*state.peek(), ScanDetailState::Loaded(detail) if detail.status == "in_progress")
                {
                    load_scan_detail(refresh_selection, selected, state, generation);
                }
            });
        }
    });
    use_effect(move || {
        if matches!(&*state.read(), ScanDetailState::Loaded(detail) if detail.status == "in_progress")
        {
            spawn(async move {
                while selected.peek().is_some()
                    && matches!(&*state.peek(), ScanDetailState::Loaded(detail) if detail.status == "in_progress")
                {
                    gloo_timers::future::TimeoutFuture::new(1_000).await;
                    now.set(Utc::now());
                }
            });
        }
    });

    let detail = match &*state.read() {
        ScanDetailState::Loaded(detail) => Some(detail.clone()),
        _ => None,
    };
    let matches = detail
        .as_ref()
        .map(|detail| diagnostic_matches(detail, &search()))
        .unwrap_or_default();
    let match_event_ids = detail
        .as_ref()
        .map(|detail| {
            matches
                .iter()
                .map(|index| detail.events[*index].id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let diagnostic_count_label = if search().trim().is_empty() {
        format!(
            "{} events",
            detail.as_ref().map_or(0, |detail| detail.events.len())
        )
    } else if matches.is_empty() {
        "0 matches".to_string()
    } else {
        format!("{} of {} matches", match_position() + 1, matches.len())
    };
    if match_position() >= matches.len() && !matches.is_empty() {
        match_position.set(matches.len() - 1);
    }

    rsx! {
        div { class: "side-panel-backdrop scanning-log-backdrop", tabindex: "-1", onclick: move |_| close_scan_detail(selected, generation),
            aside { id: "scan-diagnostics-dialog", class: "side-panel scanning-log-drawer", role: "dialog", aria_modal: "true", aria_labelledby: "scan-log-title", tabindex: "-1", onclick: move |event| event.stop_propagation(),
                DialogFocusRestore {}
                DialogInitialFocus { dialog_id: "scan-diagnostics-dialog".to_string() }
                DialogFocusSentinel { dialog_id: "scan-diagnostics-dialog".to_string(), boundary: DialogFocusBoundary::Last }
                div { class: "scanning-log-head",
                    div { h2 { id: "scan-log-title", Icon { name: IconName::Shield, size: 14 } " Scan details" } p { "{selection.label}" } code { "{selection.scan_id}" } }
                    div { class: "row-actions",
                        button { class: "btn-icon focus-ring", aria_label: "Refresh exact scan detail", onclick: { let refresh_selection = selection.clone(); move |_| load_scan_detail(refresh_selection.clone(), selected, state, generation) }, Icon { name: IconName::Sync, size: 14 } }
                        button { class: "btn-icon focus-ring", aria_label: "Close exact scan detail", onclick: move |_| close_scan_detail(selected, generation), Icon { name: IconName::X, size: 15 } }
                    }
                }
                match &*state.read() {
                    ScanDetailState::Loading => rsx! { div { class: "scanning-log-body", div { class: "q-empty", role: "status", "Loading exact scan detail…" } } },
                    ScanDetailState::Error(error) => rsx! { div { class: "scanning-log-body", { load_error_state("Exact scan detail could not be loaded", error, { let retry_selection = selection.clone(); move || load_scan_detail(retry_selection.clone(), selected, state, generation) }) } } },
                    ScanDetailState::Loaded(detail) => rsx! {
                        { detail_identity(detail) }
                        { detail_status_strip(detail, now()) }
                        { detail_callout(detail, selected, generation, retry_pending, feedback, refresh) }
                        div { class: "sd-tabs scanning-detail-tabs", role: "tablist", aria_label: "Scan detail sections",
                            onkeydown: move |event| {
                                let next = match event.key() {
                                    Key::ArrowRight | Key::ArrowLeft => match tab() {
                                        ScanDetailTab::Log => ScanDetailTab::Details,
                                        ScanDetailTab::Details => ScanDetailTab::Log,
                                    },
                                    Key::End => ScanDetailTab::Details,
                                    Key::Home => ScanDetailTab::Log,
                                    _ => return,
                                };
                                event.prevent_default();
                                tab.set(next);
                                focus_element_by_id(match next { ScanDetailTab::Log => "scan-detail-log-tab", ScanDetailTab::Details => "scan-detail-details-tab" });
                            },
                            button { id: "scan-detail-log-tab", class: if tab() == ScanDetailTab::Log { "sd-tab focus-ring active" } else { "sd-tab focus-ring" }, role: "tab", tabindex: if tab() == ScanDetailTab::Log { "0" } else { "-1" }, aria_selected: tab() == ScanDetailTab::Log, aria_controls: "scan-detail-log-panel", onclick: move |_| tab.set(ScanDetailTab::Log), "Log" }
                            button { id: "scan-detail-details-tab", class: if tab() == ScanDetailTab::Details { "sd-tab focus-ring active" } else { "sd-tab focus-ring" }, role: "tab", tabindex: if tab() == ScanDetailTab::Details { "0" } else { "-1" }, aria_selected: tab() == ScanDetailTab::Details, aria_controls: "scan-detail-details-panel", onclick: move |_| tab.set(ScanDetailTab::Details), "Details" }
                        }
                        if tab() == ScanDetailTab::Log {
                            div { id: "scan-detail-log-panel", class: "scanning-log-body scanning-detail-panel", role: "tabpanel", aria_labelledby: "scan-detail-log-tab",
                            div { class: "scanning-log-tools",
                                div { class: "q-search scanning-log-search", Icon { name: IconName::Search, size: 12 } input { class: "q-search-input", aria_label: "Search authorized diagnostic content", placeholder: "Search diagnostics…", value: search(), oninput: move |event| { search.set(event.value()); match_position.set(0); } } }
                                span { class: "filter-count", "{diagnostic_count_label}" }
                                button { class: "btn-icon focus-ring", aria_label: "Previous diagnostic match", disabled: matches.is_empty(), onclick: { let event_ids = match_event_ids.clone(); move |_| { if !event_ids.is_empty() { let position = if match_position() == 0 { event_ids.len() - 1 } else { match_position() - 1 }; match_position.set(position); scroll_to_event(event_ids[position]); } } }, Icon { name: IconName::ChevronUp, size: 13 } }
                                button { class: "btn-icon focus-ring", aria_label: "Next diagnostic match", disabled: matches.is_empty(), onclick: { let event_ids = match_event_ids.clone(); move |_| { if !event_ids.is_empty() { let position = (match_position() + 1) % event_ids.len(); match_position.set(position); scroll_to_event(event_ids[position]); } } }, Icon { name: IconName::ChevronDown, size: 13 } }
                                button { class: "btn btn-ghost xs focus-ring", onclick: { let content = diagnostic_export(detail); let filename = format!("scan-{}-diagnostics.txt", detail.scan_id); move |_| { let _ = crate::export::trigger_download(&filename, "text/plain;charset=utf-8", &content); } }, "Export current content" }
                            }
                            if detail.truncated { div { class: "sd-callout sd-callout-warning", role: "status", "The API bounded this authorized response. Export preserves the same ordered content and truncation marker." } }
                            if detail.events.is_empty() { div { class: "q-empty", h3 { "No persisted diagnostic events" } p { "No log output is fabricated for this lifecycle." } } }
                            else { div { class: "sd-log-stream build-log-stream scanning-log-stream", for (index, event) in detail.events.iter().enumerate() {
                                div { id: "scan-event-{event.id}", key: "{event.id}", class: diagnostic_line_class(event.level.as_str(), matches.get(match_position()).copied() == Some(index) && !search().trim().is_empty()),
                                    span { class: "sd-log-t", title: "{event.occurred_at.to_rfc3339()}", "{diagnostic_time(event.occurred_at)}" }
                                    span { class: "sd-log-lvl", "{event.level.to_ascii_uppercase()}" }
                                    span { class: "sd-log-m", span { class: "scanning-log-source", "{event.source}/{event.event_type} · execution {event.execution_id} · attempt {event.attempt_number} · " } { highlighted_diagnostic_message(&event.message, &search()) } }
                                    if event.truncated { span { class: "scanning-log-truncated", " Event output was truncated at the capture boundary." } }
                                }
                            } } }
                            }
                        } else {
                            div { id: "scan-detail-details-panel", class: "scanning-log-body scanning-detail-panel focus-ring", role: "tabpanel", tabindex: "0", aria_labelledby: "scan-detail-details-tab",
                                { detail_summary(detail) }
                            }
                        }
                    },
                }
                DialogFocusSentinel { dialog_id: "scan-diagnostics-dialog".to_string(), boundary: DialogFocusBoundary::First }
            }
        }
    }
}

fn detail_identity(detail: &ScanningScanDetailResponse) -> Element {
    let flake = detail.flake_name.as_deref().unwrap_or("Not recorded");
    let revision = detail.commit_hash.as_deref().unwrap_or("Not recorded");
    let short_revision = revision.chars().take(12).collect::<String>();
    rsx! {
        div { class: "scanning-detail-config",
            div { class: "scanning-detail-config-icon", Icon { name: IconName::Shield, size: 17 } }
            div { class: "scanning-detail-config-copy",
                strong { "{detail.hostname}" }
                span { class: "mono", "{flake} · {short_revision}" }
            }
        }
    }
}

fn detail_status_strip(detail: &ScanningScanDetailResponse, now: DateTime<Utc>) -> Element {
    let meta = status_meta(&detail.status);
    let elapsed = detail_elapsed_seconds(detail, now).map(format_duration);
    let trigger = detail.source_trigger.as_deref().unwrap_or("Not recorded");
    let timestamp = detail
        .completed_at
        .or(detail.started_at)
        .or(detail.scheduled_at)
        .unwrap_or(detail.created_at);
    rsx! {
        div { class: "scanning-detail-status-strip",
            span { class: "chip {meta.class}", span { class: "chip-dot", style: "background:{meta.color};" } "{meta.label}" }
            span { class: "chip chip-unknown scanning-trigger-chip", "{trigger}" }
            time { datetime: "{timestamp.to_rfc3339()}", title: "{timestamp.to_rfc3339()}", "{detail_time(timestamp)}" }
            if let Some(elapsed) = elapsed { span { class: "scanning-detail-elapsed", "Elapsed {elapsed}" } }
            if detail.archived_at.is_some() { span { class: "chip chip-unknown", "Archived" } }
            if detail.total_vulnerabilities > 0 { div { class: "scanning-detail-severity",
                if detail.critical_count > 0 { span { class: "chip chip-critical", "{detail.critical_count}C" } }
                if detail.high_count > 0 { span { class: "chip chip-warning", "{detail.high_count}H" } }
                if detail.medium_count > 0 { span { class: "chip chip-info", "{detail.medium_count}M" } }
            } }
        }
    }
}

fn detail_callout(
    detail: &ScanningScanDetailResponse,
    selected: Signal<Option<ScanDetailSelection>>,
    generation: Signal<u64>,
    retry_pending: Signal<HashSet<i32>>,
    feedback: Signal<Option<ScanActionFeedback>>,
    refresh: Signal<u64>,
) -> Element {
    let build_terminal = matches!(detail.build_status.as_deref(), Some("failed" | "cancelled"));
    let prerequisite_build_failure = detail.status == "failed"
        && detail.source_trigger.as_deref() == Some("post_build")
        && detail.attempts == 0
        && build_terminal;
    let (kind, title, guidance) = if prerequisite_build_failure {
        (
            "danger",
            if detail.build_status.as_deref() == Some("cancelled") {
                "Build cancelled before scan"
            } else {
                "Build failed before scan"
            },
            "Vulnix did not run. Review the exact prerequisite build for failure or cancellation details.",
        )
    } else if detail.status == "failed" {
        (
            "danger",
            detail
                .failure
                .as_deref()
                .unwrap_or("The vulnerability scan failed."),
            "Review the persisted scanner log. Retry starts a new exact scan for this derivation.",
        )
    } else if detail.status == "awaiting_build" && build_terminal {
        (
            "danger",
            if detail.build_status.as_deref() == Some("cancelled") {
                "The prerequisite build was cancelled"
            } else {
                "The prerequisite build failed"
            },
            "The scan remains queued and has not run Vulnix. Retry or replace the terminal build; this scan intent will continue when build output exists.",
        )
    } else if detail.status == "awaiting_build" {
        (
            "waiting",
            "Waiting on the build",
            "Vulnix needs the realized NixOS output. This scan starts automatically after the associated build succeeds.",
        )
    } else if detail.status == "awaiting_closure" {
        (
            "waiting",
            "Waiting for a reachable closure",
            "The remote build succeeded, but no completed cache publication makes its closure available to a scanner yet.",
        )
    } else {
        return rsx! {};
    };
    rsx! {
        div { class: "scanning-detail-callout scanning-detail-callout-{kind}", role: if kind == "danger" { "alert" } else { "status" },
            Icon { name: if kind == "danger" { IconName::Warn } else { IconName::Clock }, size: 15 }
            div { class: "scanning-detail-callout-copy",
                strong { "{title}" }
                p { "{guidance}" }
                div { class: "row-actions",
                    if let Some(build_job_id) = detail.build_job_id { a { class: if prerequisite_build_failure { "btn btn-primary xs focus-ring" } else { "btn btn-ghost xs focus-ring" }, href: "/builds?job={build_job_id}", Icon { name: IconName::Build, size: 11 } " View build" } }
                    if detail.status == "failed" && !prerequisite_build_failure { button { class: "btn btn-ghost xs focus-ring", disabled: retry_pending.read().contains(&detail.derivation_id), onclick: { let derivation_id = detail.derivation_id; let label = format!("{} {}", detail.hostname, commit_label(&detail.commit_hash)); move |_| { retry_exact_scan(derivation_id, label.clone(), retry_pending, feedback, refresh); close_scan_detail(selected, generation); } }, Icon { name: IconName::Sync, size: 11 } " Retry scan" } }
                }
            }
        }
    }
}

fn detail_summary(detail: &ScanningScanDetailResponse) -> Element {
    let meta = status_meta(&detail.status);
    let flake = detail.flake_name.as_deref().unwrap_or("Not recorded");
    let revision = detail.commit_hash.as_deref().unwrap_or("Not recorded");
    let trigger = detail.source_trigger.as_deref().unwrap_or("Not recorded");
    let executor = detail.executor.as_deref().unwrap_or("Not recorded");
    let scheduled = detail
        .scheduled_at
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| "Not recorded".to_string());
    let started = detail
        .started_at
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| "Not started".to_string());
    let completed = detail
        .completed_at
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| "Not terminal".to_string());
    rsx! {
        div { class: "scanning-detail-summary",
            div { class: "scanning-detail-findings",
                h3 { "Findings" }
                div { class: "scanning-detail-finding-total", strong { "{detail.total_vulnerabilities}" } span { " vulnerabilities across {detail.total_packages} packages" } }
                div { class: "scanning-detail-severity",
                    span { class: "chip chip-critical", "{detail.critical_count} critical" }
                    span { class: "chip chip-warning", "{detail.high_count} high" }
                    span { class: "chip chip-info", "{detail.medium_count} medium" }
                    span { class: "chip chip-unknown", "{detail.low_count} low" }
                }
            }
            dl { class: "scanning-detail-grid",
                dt { "Configuration" } dd { "{detail.hostname}" }
                dt { "Flake" } dd { "{flake}" }
                dt { "Revision" } dd { code { "{revision}" } }
                dt { "Status" } dd { span { class: "chip {meta.class}", "{meta.label}" } }
                dt { "Trigger" } dd { "{trigger}" }
                dt { "Scanner" } dd { "{detail.scanner_name}" if let Some(version) = detail.scanner_version.as_deref() { " {version}" } }
                dt { "Executor" } dd { "{executor}" }
                dt { "Created" } dd { time { datetime: "{detail.created_at.to_rfc3339()}", "{detail.created_at.to_rfc3339()}" } }
                dt { "Scheduled" } dd { "{scheduled}" }
                dt { "Started" } dd { "{started}" }
                dt { "Completed" } dd { "{completed}" }
                dt { "Attempts" } dd { "{detail.attempts}" }
                if let Some(build_job_id) = detail.build_job_id { dt { "Build job" } dd { code { "{build_job_id}" } if let Some(build_status) = detail.build_status.as_deref() { " · {build_status}" } } }
                if let Some(reason) = detail.wait_reason.as_deref() { dt { "Wait" } dd { "{reason}" } }
                if let Some(failure) = detail.failure.as_deref() { dt { "Failure" } dd { class: "scanning-detail-failure", "{failure}" } }
                if let Some(archived_at) = detail.archived_at { dt { "Archived" } dd { "{archived_at.to_rfc3339()}" } }
                dt { "Cancellation" } dd { if detail.cancellable { "Available" } else { "Not supported by execution ownership" } }
                dt { "Derivation" } dd { code { "{detail.derivation_id}" } }
                dt { "Scan ID" } dd { code { "{detail.scan_id}" } }
            }
        }
    }
}

fn diagnostic_line_class(level: &str, active: bool) -> &'static str {
    match (level, active) {
        ("error", true) => "sd-log-line sd-log-error log-line-hit log-line-active",
        ("warning", true) => "sd-log-line sd-log-warn log-line-hit log-line-active",
        (_, true) => "sd-log-line sd-log-info log-line-hit log-line-active",
        ("error", false) => "sd-log-line sd-log-error",
        ("warning", false) => "sd-log-line sd-log-warn",
        _ => "sd-log-line sd-log-info",
    }
}

fn diagnostic_time(occurred_at: DateTime<Utc>) -> String {
    occurred_at.format("%H:%M:%S").to_string()
}

fn detail_time(timestamp: DateTime<Utc>) -> String {
    timestamp.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn highlighted_diagnostic_message(message: &str, query: &str) -> Element {
    let query = query.trim();
    if query.is_empty() {
        return rsx! { "{message}" };
    }
    let lower_message = message.to_ascii_lowercase();
    let lower_query = query.to_ascii_lowercase();
    let mut parts = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = lower_message[cursor..].find(&lower_query) {
        let start = cursor + relative;
        if start > cursor {
            parts.push((false, message[cursor..start].to_string()));
        }
        let end = start + query.len();
        parts.push((true, message[start..end].to_string()));
        cursor = end;
    }
    if cursor < message.len() {
        parts.push((false, message[cursor..].to_string()));
    }
    rsx! { for (highlighted, part) in parts { if highlighted { mark { class: "log-hit", "{part}" } } else { "{part}" } } }
}

fn detail_elapsed_seconds(detail: &ScanningScanDetailResponse, now: DateTime<Utc>) -> Option<i64> {
    if detail.status == "in_progress" {
        detail
            .started_at
            .map(|started_at| now.signed_duration_since(started_at).num_seconds().max(0))
    } else {
        detail.scan_duration_ms.map(|ms| i64::from(ms) / 1_000)
    }
}

fn diagnostic_matches(detail: &ScanningScanDetailResponse, query: &str) -> Vec<usize> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    detail
        .events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            let haystack = format!(
                "{} {} {} {} {} {}",
                event.message,
                event.level,
                event.source,
                event.event_type,
                event.execution_id,
                event.attempt_number
            )
            .to_ascii_lowercase();
            haystack.contains(&query).then_some(index)
        })
        .collect()
}

fn diagnostic_export(detail: &ScanningScanDetailResponse) -> String {
    let mut lines = vec![
        format!("scan_id: {}", detail.scan_id),
        format!("derivation_id: {}", detail.derivation_id),
        format!("status: {}", detail.status),
        format!(
            "revision: {}",
            detail.commit_hash.as_deref().unwrap_or("not recorded")
        ),
        String::new(),
    ];
    for event in &detail.events {
        lines.push(format!(
            "{} attempt={} level={} source={} type={} execution={}{}\n{}",
            event.occurred_at.to_rfc3339(),
            event.attempt_number,
            event.level,
            event.source,
            event.event_type,
            event.execution_id,
            if event.truncated {
                " truncated=true"
            } else {
                ""
            },
            event.message
        ));
    }
    if detail.truncated {
        lines.push(
            "[response truncated: later authorized events were omitted by the API bound]"
                .to_string(),
        );
    }
    lines.join("\n")
}

#[cfg(target_arch = "wasm32")]
fn scroll_to_event(event_id: i64) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    if let Some(element) = document.get_element_by_id(&format!("scan-event-{event_id}")) {
        element.scroll_into_view();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn scroll_to_event(_event_id: i64) {}

#[allow(clippy::too_many_arguments)]
fn schedule_modal(
    policy: Option<ScanSchedulePolicyResponse>,
    load_error: Option<String>,
    mut open: Signal<bool>,
    policy_on_build: Signal<bool>,
    policy_deployed_interval: Signal<String>,
    policy_recent_interval: Signal<String>,
    policy_archived_interval: Signal<String>,
    policy_archived_enabled: Signal<bool>,
    policy_rebuild_to_scan: Signal<bool>,
    mut save_error: Signal<Option<String>>,
    mut saving: Signal<bool>,
    mut refresh: Signal<u64>,
    selected_scan: Signal<Option<ScanDetailSelection>>,
    retry: impl FnMut() + 'static,
) -> Element {
    let mut retry = retry;
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| open.set(false),
            div { id: "scan-schedule-dialog", class: "modal scanning-schedule-modal", role: "dialog", aria_modal: "true", aria_labelledby: "scan-schedule-title", tabindex: "-1", onclick: move |event| event.stop_propagation(), onkeydown: move |event| if event.key() == Key::Escape && selected_scan.peek().is_none() { event.stop_propagation(); open.set(false); },
                DialogFocusRestore {}
                DialogInitialFocus { dialog_id: "scan-schedule-dialog".to_string() }
                DialogFocusSentinel { dialog_id: "scan-schedule-dialog".to_string(), boundary: DialogFocusBoundary::Last }
                div { class: "modal-head", h2 { id: "scan-schedule-title", Icon { name: IconName::Gear, size: 14 } " Scan schedule" } p { "Edit the persisted server scan policy." } }
                div { class: "modal-body",
                    if let Some(error) = load_error { { load_error_state("The scan schedule could not be loaded", &error, move || retry()) } }
                    else if policy.is_none() { div { class: "scanning-modal-state", role: "status", "Loading schedule…" } }
                    else { div { class: "scanning-schedule-rows",
                        if let Some(error) = save_error() { div { class: "sd-callout sd-callout-danger", role: "alert", "{error}" } }
                        { schedule_row("Scan on build", "Scan a freshly built exact configuration before deployment.", bool_control(policy_on_build, "Scan on build")) }
                        { schedule_row("Deployed configs", "Rescan currently running configurations.", interval_select(policy_deployed_interval, false, "Deployed configs scan interval")) }
                        { schedule_row("Recent configs", "Rescan recent configurations that are not deployed.", interval_select(policy_recent_interval, false, "Recent configs scan interval")) }
                        { schedule_row("Superseded configs", "Reduce work for superseded configurations.", archive_control(policy_archived_enabled, policy_archived_interval)) }
                        { schedule_row("Rebuild to scan old configs", "Permit policy-driven rebuilds when an archived closure is unavailable.", bool_control(policy_rebuild_to_scan, "Rebuild to scan old configs")) }
                    } }
                }
                div { class: "modal-foot",
                    button { class: "btn btn-ghost focus-ring", disabled: saving(), onclick: move |_| open.set(false), "Cancel" }
                    button { class: "btn btn-primary focus-ring", disabled: policy.is_none() || saving(), onclick: move |_| {
                        let request = UpdateScanSchedulePolicyRequest { on_build: policy_on_build(), deployed_interval: policy_deployed_interval(), recent_interval: policy_recent_interval(), archived_interval: policy_archived_interval(), archived_enabled: policy_archived_enabled(), rebuild_to_scan: policy_rebuild_to_scan() };
                        save_error.set(None); saving.set(true);
                        spawn(async move { match update_scanning_schedule(&request).await { Ok(_) => { refresh.set(refresh().wrapping_add(1)); open.set(false); }, Err(error) => save_error.set(Some(format!("The scan schedule could not be saved: {error}"))) } saving.set(false); });
                    }, Icon { name: IconName::Check, size: 13 } if saving() { " Saving…" } else { " Save schedule" } }
                }
                DialogFocusSentinel { dialog_id: "scan-schedule-dialog".to_string(), boundary: DialogFocusBoundary::First }
            }
        }
    }
}

fn bool_control(mut value: Signal<bool>, label: &'static str) -> Element {
    rsx! { label { class: "scanning-toggle", input { r#type: "checkbox", aria_label: "{label}", checked: value(), onchange: move |event| value.set(event.checked()) } span { if value() { "On" } else { "Off" } } } }
}

fn archive_control(enabled: Signal<bool>, interval: Signal<String>) -> Element {
    rsx! { div { class: "scanning-archive-control", { bool_control(enabled, "Scan superseded configs") } { interval_select(interval, !enabled(), "Superseded configs scan interval") } } }
}

fn interval_select(mut value: Signal<String>, disabled: bool, label: &'static str) -> Element {
    rsx! { select { class: "input focus-ring", aria_label: "{label}", disabled, value: value(), oninput: move |event| value.set(event.value()), for option in ["1h", "6h", "12h", "24h", "7d", "30d", "168h", "336h", "never"] { option { value: "{option}", if option == "never" { "Never" } else { "Every {option}" } } } } }
}

fn schedule_row(title: &str, description: &str, control: Element) -> Element {
    rsx! { div { class: "scanning-schedule-row", div { div { class: "scanning-schedule-title", "{title}" } div { class: "scanning-schedule-description", "{description}" } } div { class: "scanning-schedule-control", {control} } } }
}

fn load_error_state(title: &str, error: &str, retry: impl FnMut() + 'static) -> Element {
    let mut retry = retry;
    rsx! { div { class: "q-empty", role: "alert", Icon { name: IconName::Warn, size: 20 } h3 { "{title}" } p { "{error}" } button { class: "btn btn-ghost xs focus-ring", onclick: move |_| retry(), "Retry" } } }
}

fn findings(critical: i32, high: i32, medium: i32, low: i32, authoritative: bool) -> Element {
    if !authoritative {
        return rsx! { span { class: "scanning-unavailable", "Not available" } };
    }
    rsx! { div { class: "scanning-findings",
        if critical > 0 { span { class: "chip chip-critical", "{critical}C" } }
        if high > 0 { span { class: "chip chip-warning", "{high}H" } }
        if medium > 0 { span { class: "chip chip-info", "{medium}M" } }
        if low > 0 { span { class: "chip chip-unknown", "{low}L" } }
        if critical + high + medium + low == 0 { span { class: "chip chip-healthy", Icon { name: IconName::Check, size: 9 } " clean" } }
    } }
}

fn relative_time(timestamp: DateTime<Utc>) -> String {
    let age = Utc::now().signed_duration_since(timestamp);
    if age.num_minutes() < 1 {
        "just now".to_string()
    } else if age.num_hours() < 1 {
        format!("{}m ago", age.num_minutes())
    } else if age.num_days() < 1 {
        format!("{}h ago", age.num_hours())
    } else {
        format!("{}d ago", age.num_days())
    }
}

fn format_duration(seconds: i64) -> String {
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}h {minutes:02}m {seconds:02}s")
    } else {
        format!("{minutes}m {seconds:02}s")
    }
}

fn commit_label(commit_hash: &Option<String>) -> String {
    commit_hash
        .as_deref()
        .filter(|hash| !hash.is_empty())
        .map(|hash| hash.chars().take(12).collect())
        .unwrap_or_else(|| "unknown".to_string())
}

fn coverage_width(count: i64, total: i64) -> String {
    if total <= 0 {
        return "0%".to_string();
    }
    format!("{:.2}%", (count.max(0) as f64 / total as f64) * 100.0)
}

fn stat_card(label: &str, value: &str, meta: Option<&str>, color: &str) -> Element {
    rsx! { div { class: "stat", span { class: "stat-accent", style: "--stat-color:{color};" } div { class: "stat-label", "{label}" } div { class: "stat-value", style: "color:{color};", "{value}" } if let Some(meta) = meta { div { class: "stat-meta", "{meta}" } } } }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;
    use crate::api::models::ScanningScanDiagnosticEventResponse;

    fn ids(count: usize) -> Vec<Uuid> {
        (1..=count)
            .map(|value| Uuid::from_u128(value as u128))
            .collect()
    }

    fn selection(values: &[Uuid]) -> HashSet<Uuid> {
        values.iter().copied().collect()
    }

    #[test]
    fn completed_shift_selects_forward_inclusive_loaded_range() {
        let ordered = ids(4);
        let (selected, anchor) = completed_range_selection(
            &HashSet::new(),
            Some(ordered[1]),
            ordered[3],
            &ordered,
            CompletedSelectionGesture::ShiftRange,
        );
        assert_eq!(selected, selection(&ordered[1..=3]));
        assert_eq!(anchor, Some(ordered[1]));
    }

    #[test]
    fn completed_shift_selects_reverse_inclusive_loaded_range() {
        let ordered = ids(4);
        let (selected, anchor) = completed_range_selection(
            &HashSet::new(),
            Some(ordered[3]),
            ordered[1],
            &ordered,
            CompletedSelectionGesture::ShiftRange,
        );
        assert_eq!(selected, selection(&ordered[1..=3]));
        assert_eq!(anchor, Some(ordered[3]));
    }

    #[test]
    fn completed_shift_without_anchor_selects_clicked_and_sets_anchor() {
        let ordered = ids(4);
        let clicked = ordered[2];
        let (selected, anchor) = completed_range_selection(
            &HashSet::new(),
            None,
            clicked,
            &ordered,
            CompletedSelectionGesture::ShiftRange,
        );
        assert_eq!(selected, selection(&[clicked]));
        assert_eq!(anchor, Some(clicked));
    }

    #[test]
    fn completed_shift_with_stale_anchor_falls_back_to_clicked_row() {
        let ordered = ids(4);
        let stale_anchor = Uuid::from_u128(99);
        let clicked = ordered[2];
        let (selected, anchor) = completed_range_selection(
            &HashSet::new(),
            Some(stale_anchor),
            clicked,
            &ordered,
            CompletedSelectionGesture::ShiftRange,
        );
        assert_eq!(selected, selection(&[clicked]));
        assert_eq!(anchor, Some(clicked));
    }

    #[test]
    fn completed_modifier_toggle_deselects_only_clicked_row() {
        let ordered = ids(4);
        let (selected, anchor) = completed_range_selection(
            &selection(&[ordered[0], ordered[1], ordered[2]]),
            Some(ordered[0]),
            ordered[1],
            &ordered,
            CompletedSelectionGesture::ModifierToggle,
        );
        assert_eq!(selected, selection(&[ordered[0], ordered[2]]));
        assert_eq!(anchor, Some(ordered[1]));
    }

    #[test]
    fn completed_modifier_toggle_selects_unselected_row_and_anchors_it() {
        let ordered = ids(4);
        let (selected, anchor) = completed_range_selection(
            &selection(&[ordered[0]]),
            Some(ordered[0]),
            ordered[2],
            &ordered,
            CompletedSelectionGesture::ModifierToggle,
        );
        assert_eq!(selected, selection(&[ordered[0], ordered[2]]));
        assert_eq!(anchor, Some(ordered[2]));
    }

    #[test]
    fn completed_shift_adds_range_without_clearing_outside_selection() {
        let ordered = ids(5);
        let outside = Uuid::from_u128(99);
        let (selected, anchor) = completed_range_selection(
            &selection(&[outside]),
            Some(ordered[1]),
            ordered[3],
            &ordered,
            CompletedSelectionGesture::ShiftRange,
        );
        assert_eq!(
            selected,
            selection(&[outside, ordered[1], ordered[2], ordered[3]])
        );
        assert_eq!(anchor, Some(ordered[1]));
    }

    #[test]
    fn completed_shift_range_is_not_truncated_to_archive_batch_limit() {
        let ordered = ids(101);
        let (selected, anchor) = completed_range_selection(
            &HashSet::new(),
            Some(ordered[0]),
            ordered[100],
            &ordered,
            CompletedSelectionGesture::ShiftRange,
        );
        assert_eq!(selected.len(), 101);
        assert_eq!(selected, selection(&ordered));
        assert_eq!(anchor, Some(ordered[0]));
    }

    #[test]
    fn completed_checkbox_applies_exact_checked_state_and_sets_anchor() {
        let ordered = ids(3);
        let (selected, anchor) = completed_range_selection(
            &selection(&ordered),
            Some(ordered[0]),
            ordered[1],
            &ordered,
            CompletedSelectionGesture::Checkbox { checked: false },
        );
        assert_eq!(selected, selection(&[ordered[0], ordered[2]]));
        assert_eq!(anchor, Some(ordered[1]));
    }

    fn row(
        hostname: &str,
        status: &str,
        hours_ago: i64,
        critical: i32,
    ) -> ScanningScanRecordResponse {
        let timestamp = Utc::now() - Duration::hours(hours_ago);
        ScanningScanRecordResponse {
            scan_id: Uuid::new_v4(),
            derivation_id: critical + hours_ago as i32 + 1,
            hostname: hostname.to_string(),
            flake_name: Some("infra".to_string()),
            commit_hash: Some(format!("{hostname}-{hours_ago:02}-full-revision")),
            is_current: false,
            is_latest_per_flake: false,
            status: status.to_string(),
            source_trigger: Some("manual".to_string()),
            created_at: timestamp,
            scheduled_at: Some(timestamp),
            started_at: (status == "in_progress").then_some(timestamp),
            completed_at: matches!(status, "completed" | "failed").then_some(timestamp),
            scanner_name: "vulnix".to_string(),
            scanner_version: None,
            executor: None,
            failure: (status == "failed").then(|| "failure".to_string()),
            wait_reason: status
                .starts_with("awaiting")
                .then(|| "prerequisite".to_string()),
            total_packages: 10,
            total_vulnerabilities: critical,
            critical_count: critical,
            high_count: 0,
            medium_count: 0,
            low_count: 0,
            scan_duration_ms: Some(1_000),
            attempts: 1,
            archived_at: None,
            cancellable: false,
        }
    }

    fn detail(
        events: Vec<ScanningScanDiagnosticEventResponse>,
        truncated: bool,
    ) -> ScanningScanDetailResponse {
        let row = row("atlas", "failed", 1, 1);
        ScanningScanDetailResponse {
            scan_id: row.scan_id,
            derivation_id: row.derivation_id,
            hostname: row.hostname,
            flake_name: row.flake_name,
            commit_hash: row.commit_hash,
            status: row.status,
            scanner_name: row.scanner_name,
            scanner_version: row.scanner_version,
            source_trigger: row.source_trigger,
            created_at: row.created_at,
            scheduled_at: row.scheduled_at,
            started_at: row.started_at,
            completed_at: row.completed_at,
            scan_duration_ms: row.scan_duration_ms,
            attempts: row.attempts,
            total_packages: row.total_packages,
            total_vulnerabilities: row.total_vulnerabilities,
            critical_count: row.critical_count,
            high_count: row.high_count,
            medium_count: row.medium_count,
            low_count: row.low_count,
            failure: row.failure,
            wait_reason: row.wait_reason,
            build_job_id: Some(Uuid::new_v4()),
            build_status: Some("failed".to_string()),
            executor: row.executor,
            archived_at: row.archived_at,
            cancellable: false,
            events,
            truncated,
        }
    }

    fn page(
        items: Vec<ScanningScanRecordResponse>,
        total: i64,
        hidden_archived: i64,
        next_cursor: Option<&str>,
    ) -> ScanningScanRecordsResponse {
        ScanningScanRecordsResponse {
            items,
            total,
            hidden_archived,
            has_more: next_cursor.is_some(),
            next_cursor: next_cursor.map(ToString::to_string),
        }
    }

    fn completed_request() -> CompletedRequest {
        CompletedRequest {
            include_archived: false,
            search: String::new(),
            status: ScanRecordStatusParam::All,
            revision: ScanRecordRevisionParam::All,
            latest_only: false,
            sort: ScanRecordSortParam::Timestamp,
            direction: ScanRecordDirectionParam::Desc,
        }
    }

    fn unscanned_derivation(rescan_eligible: bool) -> ScanningQueueItemResponse {
        ScanningQueueItemResponse {
            derivation_id: 42,
            rescan_eligible,
            scan_id: None,
            hostname: "atlas".to_string(),
            flake_name: Some("infra".to_string()),
            commit_hash: Some("full-unscanned-revision".to_string()),
            status: "never_scanned".to_string(),
            completed_at: None,
            scheduled_at: None,
            critical_count: 0,
            high_count: 0,
            medium_count: 0,
            freshness: "archived".to_string(),
            is_current: true,
            is_latest_per_flake: true,
            source_trigger: None,
        }
    }

    #[test]
    fn classifies_all_authoritative_active_states() {
        assert_eq!(status_meta("in_progress").label, "Scanning");
        assert_eq!(status_meta("pending").label, "Queued");
        assert_eq!(status_meta("awaiting_build").label, "Awaiting build");
        assert_eq!(status_meta("awaiting_closure").label, "Awaiting closure");
    }

    #[test]
    fn filters_and_sorts_with_domain_values_and_stable_identity() {
        let rows = vec![row("zeta", "completed", 3, 0), row("atlas", "failed", 1, 2)];
        let filtered = filter_and_sort_active_records(
            &rows,
            "atlas",
            "failed",
            "all",
            false,
            ScanSort::Timestamp,
            true,
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].hostname, "atlas");
        let sorted = filter_and_sort_active_records(
            &rows,
            "",
            "all",
            "all",
            false,
            ScanSort::Severity,
            true,
        );
        assert_eq!(sorted[0].hostname, "atlas");
    }

    #[test]
    fn latest_filter_uses_server_revision_authority_not_scan_time() {
        let mut older_commit_rescanned_today = row("older", "completed", 0, 0);
        older_commit_rescanned_today.is_latest_per_flake = false;
        let mut newer_commit_scanned_yesterday = row("newer", "completed", 24, 0);
        newer_commit_scanned_yesterday.is_latest_per_flake = true;

        let filtered = filter_and_sort_active_records(
            &[
                older_commit_rescanned_today,
                newer_commit_scanned_yesterday.clone(),
            ],
            "",
            "all",
            "all",
            true,
            ScanSort::Timestamp,
            true,
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].scan_id, newer_commit_scanned_yesterday.scan_id);
    }

    #[test]
    fn severity_sort_compares_each_severity_without_weight_collisions() {
        let mut one_critical = row("critical", "completed", 1, 1);
        one_critical.high_count = 0;
        let mut many_high = row("high", "completed", 1, 0);
        many_high.high_count = 1_000;

        let sorted = filter_and_sort_active_records(
            &[many_high, one_critical],
            "",
            "all",
            "all",
            false,
            ScanSort::Severity,
            true,
        );

        assert_eq!(sorted[0].hostname, "critical");
    }

    #[test]
    fn failed_stat_asks_the_server_for_the_newest_failure() {
        let query = newest_failed_query();
        assert_eq!(query.collection.as_param(), "completed");
        assert_eq!(query.status.as_param(), "failed");
        assert_eq!(query.sort.as_param(), "timestamp");
        assert_eq!(query.direction.as_param(), "desc");
        assert_eq!(query.limit, 1);
        assert!(
            query.include_archived,
            "archived evidence is retained and must remain reachable",
        );
        assert!(
            query.after.is_none() && query.search.is_none(),
            "the newest-failure lookup must not depend on view state",
        );
    }

    #[test]
    fn prerequisite_build_failures_are_not_scan_retries() {
        let mut failed = row("atlas", "failed", 1, 0);
        failed.source_trigger = Some("post_build".to_string());
        failed.attempts = 0;
        assert!(is_prerequisite_build_failure(&failed));

        failed.attempts = 1;
        assert!(!is_prerequisite_build_failure(&failed));
        failed.attempts = 0;
        failed.source_trigger = Some("manual".to_string());
        assert!(!is_prerequisite_build_failure(&failed));
    }

    #[test]
    fn failure_preview_is_unicode_safe_and_bounded() {
        let preview = bounded_failure_preview(&"å".repeat(121));
        assert_eq!(preview.chars().count(), 121);
        assert!(preview.ends_with('…'));
        assert_eq!(bounded_failure_preview("short failure"), "short failure");
    }

    #[test]
    fn diagnostic_search_and_export_preserve_authorized_order_and_bounds() {
        let event = |id, message: &str| ScanningScanDiagnosticEventResponse {
            id,
            execution_id: Uuid::new_v4(),
            attempt_number: 1,
            occurred_at: Utc::now(),
            level: "error".to_string(),
            source: "vulnix".to_string(),
            event_type: "output".to_string(),
            message: message.to_string(),
            truncated: false,
        };
        let detail = detail(vec![event(1, "first needle"), event(2, "second")], true);
        assert_eq!(diagnostic_matches(&detail, "needle"), vec![0]);
        let export = diagnostic_export(&detail);
        assert!(export.find("first needle").unwrap() < export.find("second").unwrap());
        assert!(export.contains("response truncated"));
    }

    #[test]
    fn detail_requests_reject_stale_generations() {
        let scan_id = Uuid::new_v4();
        let selection = ScanDetailSelection {
            scan_id,
            label: "scan".to_string(),
        };
        assert!(!scan_detail_request_is_current(
            ScanDetailRequest {
                scan_id,
                generation: 1
            },
            Some(&selection),
            2
        ));
        assert!(scan_detail_request_is_current(
            ScanDetailRequest {
                scan_id,
                generation: 2
            },
            Some(&selection),
            2
        ));
    }

    #[test]
    fn archive_aware_totals_distinguish_visible_and_all_rows() {
        let without_archived = page(vec![row("atlas", "completed", 1, 0)], 12, 5, None);
        let with_archived = page(vec![row("atlas", "completed", 1, 0)], 12, 0, None);
        assert_eq!(visible_record_total(&without_archived), 7);
        assert_eq!(visible_record_total(&with_archived), 12);

        let mut state = ScanRecordPaginationState::default();
        state.reset(Some(without_archived));
        assert_eq!(state.hidden_archived(), 5);
        assert_eq!(state.visible_total(), 7);
        assert_ne!(
            state.hidden_archived(),
            state.rows().len() as i64,
            "the archived count must come from the server, not the loaded rows",
        );
    }

    #[test]
    fn by_system_archive_visibility_is_independent_per_system() {
        let alpha = Uuid::from_u128(1);
        let beta = Uuid::from_u128(2);
        let mut states = HashMap::new();

        set_system_archive_visibility(&mut states, alpha, true);
        assert!(system_archive_visibility(&states, alpha));
        assert!(!system_archive_visibility(&states, beta));

        set_system_archive_visibility(&mut states, beta, true);
        set_system_archive_visibility(&mut states, alpha, false);
        assert!(!system_archive_visibility(&states, alpha));
        assert!(system_archive_visibility(&states, beta));
    }

    #[test]
    fn system_history_includes_unscanned_derivations_without_scan_identity() {
        let entry = system_history_entries(SystemHistoryData {
            scans: page(Vec::new(), 0, 0, None),
            derivations: vec![unscanned_derivation(false)],
        })
        .pop()
        .expect("the no-scan derivation should remain visible");
        assert!(
            matches!(entry, SystemHistoryEntry::NoScan(row) if row.scan_id.is_none() && !row.rescan_eligible)
        );
    }

    #[test]
    fn running_elapsed_requires_authoritative_start_time() {
        let mut running = detail(Vec::new(), false);
        running.status = "in_progress".to_string();
        running.created_at = Utc::now() - Duration::hours(3);
        running.started_at = None;
        assert_eq!(detail_elapsed_seconds(&running, Utc::now()), None);

        let now = Utc::now();
        running.started_at = Some(now - Duration::seconds(75));
        assert_eq!(detail_elapsed_seconds(&running, now), Some(75));
    }

    #[test]
    fn continuation_appends_unique_rows_and_advances_the_cursor() {
        let first = row("alpha", "completed", 1, 0);
        let second = row("beta", "completed", 2, 0);
        let mut state = ScanRecordPaginationState::default();
        state.reset(Some(page(vec![first.clone()], 2, 0, Some("cursor-1"))));

        let request = state
            .begin_continuation()
            .expect("a next cursor must start one continuation");
        assert_eq!(request.cursor, "cursor-1");
        assert!(
            state.begin_continuation().is_none(),
            "one continuation must be active at a time",
        );
        assert!(state.complete_continuation(
            &request,
            page(vec![first.clone(), second.clone()], 2, 0, None),
        ));

        let loaded = state
            .rows()
            .iter()
            .map(|row| row.scan_id)
            .collect::<Vec<_>>();
        assert_eq!(loaded, vec![first.scan_id, second.scan_id]);
        assert!(!state.has_more());
        assert!(state.begin_continuation().is_none());
    }

    #[test]
    fn continuation_responses_from_superseded_requests_are_ignored() {
        let loaded = row("alpha", "completed", 1, 0);
        let stale = row("stale", "completed", 9, 0);
        let mut state = ScanRecordPaginationState::default();
        state.reset(Some(page(vec![loaded.clone()], 5, 0, Some("cursor-1"))));
        let request = state.begin_continuation().expect("continuation starts");

        // A filter change resets the state while the request is in flight.
        state.reset(Some(page(vec![loaded.clone()], 5, 0, Some("cursor-2"))));
        assert!(state.complete_continuation(&request, page(vec![stale], 5, 0, None)));

        assert_eq!(state.rows().len(), 1);
        assert_eq!(state.rows()[0].scan_id, loaded.scan_id);
        assert!(
            state.has_more(),
            "the superseded response must not overwrite the current cursor",
        );
    }

    #[test]
    fn continuation_failure_keeps_loaded_rows_and_allows_retry() {
        let loaded = row("alpha", "completed", 1, 0);
        let next = row("beta", "completed", 2, 0);
        let mut state = ScanRecordPaginationState::default();
        state.reset(Some(page(vec![loaded.clone()], 2, 0, Some("cursor-1"))));
        let request = state.begin_continuation().expect("continuation starts");

        state.fail_continuation(&request, "HTTP 500: unavailable".into());
        assert_eq!(
            state.continuation_error.as_deref(),
            Some("HTTP 500: unavailable"),
        );
        assert_eq!(state.rows().len(), 1);

        let retry = state
            .begin_continuation()
            .expect("a failed continuation must stay retryable");
        assert_eq!(retry.cursor, "cursor-1");
        assert!(state.continuation_error.is_none());
        assert!(state.complete_continuation(&retry, page(vec![next.clone()], 2, 0, None)));
        assert_eq!(state.rows().len(), 2);
    }

    #[test]
    fn head_refresh_prepends_new_rows_without_refetching_loaded_pages() {
        let newest = row("newest", "completed", 0, 0);
        let first = row("alpha", "completed", 1, 0);
        let second = row("beta", "completed", 2, 0);
        let mut state = ScanRecordPaginationState::default();
        state.reset(Some(page(vec![first.clone()], 2, 0, Some("cursor-1"))));
        let request = state.begin_continuation().expect("continuation starts");
        assert!(
            state.complete_continuation(
                &request,
                page(vec![second.clone()], 2, 0, Some("cursor-2")),
            )
        );

        let token = state.begin_head_refresh().expect("head refresh starts");
        assert!(state.complete_head_refresh(
            &token,
            page(
                vec![newest.clone(), first.clone()],
                3,
                0,
                Some("cursor-head"),
            ),
            HeadRefreshMode::PrependNewRows,
        ));

        let loaded = state
            .rows()
            .iter()
            .map(|row| row.scan_id)
            .collect::<Vec<_>>();
        assert_eq!(
            loaded,
            vec![newest.scan_id, first.scan_id, second.scan_id],
            "the refreshed head must not duplicate or reorder accumulated rows",
        );
        assert_eq!(state.total(), 3);
        assert_eq!(
            state.begin_continuation().map(|request| request.cursor),
            Some("cursor-2".to_string()),
            "continuation must resume after the accumulated tail",
        );
    }

    #[test]
    fn continuation_keeps_newer_head_totals() {
        let newest = row("newest", "completed", 0, 0);
        let first = row("alpha", "completed", 1, 0);
        let second = row("beta", "completed", 2, 0);
        let third = row("gamma", "completed", 3, 0);
        let mut state = ScanRecordPaginationState::default();
        state.reset(Some(page(vec![first.clone()], 3, 0, Some("cursor-1"))));
        let request = state.begin_continuation().expect("continuation starts");
        assert!(state.complete_continuation(&request, page(vec![second], 3, 0, Some("cursor-2")),));

        let refresh = state.begin_head_refresh().expect("head refresh starts");
        assert!(state.complete_head_refresh(
            &refresh,
            page(vec![newest, first], 4, 0, Some("new-head-cursor")),
            HeadRefreshMode::PrependNewRows,
        ));
        assert_eq!(state.total(), 4);

        let continuation = state
            .begin_continuation()
            .expect("old cursor remains valid");
        assert_eq!(continuation.cursor, "cursor-2");
        assert!(state.complete_continuation(&continuation, page(vec![third], 3, 0, None),));
        assert_eq!(
            state.total(),
            4,
            "old snapshot totals must not replace the refreshed head total"
        );
    }

    #[test]
    fn head_refresh_allows_only_one_request_and_retries_after_failure() {
        let first = row("alpha", "completed", 1, 0);
        let mut state = ScanRecordPaginationState::default();
        state.reset(Some(page(vec![first], 1, 0, None)));

        let refresh = state.begin_head_refresh().expect("first refresh starts");
        assert!(state.begin_head_refresh().is_none());
        state.fail_head_refresh(&refresh);
        assert!(state.begin_head_refresh().is_some());
    }

    #[test]
    fn continuation_waits_for_an_active_head_refresh() {
        let mut state = ScanRecordPaginationState::default();
        state.reset(Some(page(
            vec![row("alpha", "completed", 1, 0)],
            2,
            0,
            Some("cursor-1"),
        )));
        let _refresh = state.begin_head_refresh().expect("head refresh starts");

        assert!(
            state.begin_continuation().is_none(),
            "a continuation must not race a head-page replacement",
        );
    }

    #[test]
    fn head_refresh_keeps_order_for_non_newest_first_requests() {
        let unseen = row("unseen", "completed", 0, 0);
        let first = row("alpha", "completed", 1, 0);
        let second = row("beta", "completed", 2, 0);
        let mut state = ScanRecordPaginationState::default();
        state.reset(Some(page(vec![first.clone()], 9, 0, Some("cursor-1"))));
        let request = state.begin_continuation().expect("continuation starts");
        assert!(state.complete_continuation(&request, page(vec![second.clone()], 9, 0, None)));

        let token = state.begin_head_refresh().expect("head refresh starts");
        assert!(state.complete_head_refresh(
            &token,
            page(vec![unseen, first.clone()], 9, 0, None),
            HeadRefreshMode::UpdateLoadedOnly,
        ));

        let loaded = state
            .rows()
            .iter()
            .map(|row| row.scan_id)
            .collect::<Vec<_>>();
        assert_eq!(loaded, vec![first.scan_id, second.scan_id]);
    }

    #[test]
    fn non_newest_head_refresh_restarts_when_the_collection_grows() {
        let unseen = row("unseen", "completed", 0, 0);
        let first = row("alpha", "completed", 1, 0);
        let second = row("beta", "completed", 2, 0);
        let mut state = ScanRecordPaginationState::default();
        state.reset(Some(page(vec![first.clone()], 2, 0, Some("cursor-1"))));
        let request = state.begin_continuation().expect("continuation starts");
        assert!(state.complete_continuation(&request, page(vec![second], 2, 0, None)));

        let token = state.begin_head_refresh().expect("head refresh starts");
        assert!(state.complete_head_refresh(
            &token,
            page(
                vec![unseen.clone(), first.clone()],
                3,
                0,
                Some("fresh-cursor"),
            ),
            HeadRefreshMode::UpdateLoadedOnly,
        ));

        assert_eq!(
            state
                .rows()
                .iter()
                .map(|row| row.scan_id)
                .collect::<Vec<_>>(),
            vec![unseen.scan_id, first.scan_id],
        );
        assert_eq!(
            state.begin_continuation().map(|request| request.cursor),
            Some("fresh-cursor".to_string()),
            "an arbitrary-order refresh must discard the old high-water cursor",
        );
    }

    #[test]
    fn newest_head_refresh_restarts_when_new_rows_exceed_the_head_page() {
        let newest = row("newest", "completed", 0, 0);
        let first = row("alpha", "completed", 1, 0);
        let second = row("beta", "completed", 2, 0);
        let mut state = ScanRecordPaginationState::default();
        state.reset(Some(page(vec![first], 2, 0, Some("cursor-1"))));
        let request = state.begin_continuation().expect("continuation starts");
        assert!(state.complete_continuation(&request, page(vec![second], 2, 0, None)));

        let token = state.begin_head_refresh().expect("head refresh starts");
        assert!(state.complete_head_refresh(
            &token,
            page(vec![newest.clone()], 103, 0, Some("fresh-cursor")),
            HeadRefreshMode::PrependNewRows,
        ));

        assert_eq!(state.rows().len(), 1);
        assert_eq!(state.rows()[0].scan_id, newest.scan_id);
        assert_eq!(
            state.begin_continuation().map(|request| request.cursor),
            Some("fresh-cursor".to_string()),
            "a truncated refreshed head must replace the old high-water cursor",
        );
    }

    #[test]
    fn head_refresh_restarts_when_archive_visibility_changes() {
        let first = row("alpha", "completed", 1, 0);
        let second = row("beta", "completed", 2, 0);
        let mut state = ScanRecordPaginationState::default();
        state.reset(Some(page(vec![first], 2, 0, Some("cursor-1"))));
        let request = state.begin_continuation().expect("continuation starts");
        assert!(state.complete_continuation(&request, page(vec![second.clone()], 2, 0, None),));

        let token = state.begin_head_refresh().expect("head refresh starts");
        assert!(state.complete_head_refresh(
            &token,
            page(vec![second.clone()], 2, 1, Some("fresh-cursor")),
            HeadRefreshMode::PrependNewRows,
        ));

        assert_eq!(state.rows().len(), 1);
        assert_eq!(state.rows()[0].scan_id, second.scan_id);
        assert_eq!(state.hidden_archived(), 1);
        assert_eq!(
            state.begin_continuation().map(|request| request.cursor),
            Some("fresh-cursor".to_string()),
        );
    }

    #[test]
    fn head_refresh_updates_loaded_rows_in_place_when_visibility_is_unchanged() {
        let mut archived = row("alpha", "completed", 1, 0);
        let mut state = ScanRecordPaginationState::default();
        state.reset(Some(page(vec![archived.clone()], 2, 0, Some("cursor-1"))));
        let request = state.begin_continuation().expect("continuation starts");
        assert!(state.complete_continuation(
            &request,
            page(vec![row("beta", "completed", 2, 0)], 2, 0, None),
        ));

        archived.archived_at = Some(Utc::now());
        let token = state.begin_head_refresh().expect("head refresh starts");
        assert!(state.complete_head_refresh(
            &token,
            page(vec![archived.clone()], 2, 0, None),
            HeadRefreshMode::PrependNewRows,
        ));

        assert_eq!(state.rows().len(), 2);
        assert!(state.rows()[0].archived_at.is_some());
    }

    #[test]
    fn head_refresh_waits_for_an_active_continuation() {
        let mut state = ScanRecordPaginationState::default();
        state.reset(Some(page(
            vec![row("alpha", "completed", 1, 0)],
            2,
            0,
            Some("cursor-1"),
        )));
        let _request = state.begin_continuation().expect("continuation starts");
        assert!(
            state.begin_head_refresh().is_none(),
            "merging a head page during a continuation could duplicate rows",
        );
    }

    #[test]
    fn completed_requests_page_the_server_and_restart_without_a_cursor() {
        let request = completed_request();
        let head = request.to_query(None);
        assert_eq!(head.collection.as_param(), "completed");
        assert_eq!(head.limit, COMPLETED_PAGE_LIMIT);
        assert!(head.after.is_none());
        assert!(head.search.is_none());

        let continuation = request.to_query(Some("cursor-1".to_string()));
        assert_eq!(continuation.after.as_deref(), Some("cursor-1"));
        assert_eq!(continuation.limit, head.limit);

        let mut narrowed = completed_request();
        narrowed.search = "gray".to_string();
        narrowed.status = ScanRecordStatusParam::Failed;
        let narrowed_query = narrowed.to_query(None);
        assert_eq!(narrowed_query.search.as_deref(), Some("gray"));
        assert_eq!(narrowed_query.status.as_param(), "failed");
        assert!(
            narrowed_query.after.is_none(),
            "a changed filter must restart pagination without a cursor",
        );
        assert_ne!(
            narrowed.reset_key(),
            request.reset_key(),
            "a changed filter must reset scroll paging",
        );
        assert!(narrowed.has_narrowing_filter());
        assert!(!request.has_narrowing_filter());
    }

    #[test]
    fn archived_probe_counts_hidden_rows_without_transferring_them() {
        let mut request = completed_request();
        request.include_archived = true;
        request.search = "gray".to_string();
        let probe = request.archived_count_query();
        assert!(!probe.include_archived);
        assert_eq!(probe.limit, 1);
        assert_eq!(probe.search.as_deref(), Some("gray"));
        assert!(probe.after.is_none());
    }

    #[test]
    fn only_newest_first_requests_may_prepend_refreshed_head_rows() {
        let mut request = completed_request();
        assert_eq!(request.head_refresh_mode(), HeadRefreshMode::PrependNewRows);

        request.direction = ScanRecordDirectionParam::Asc;
        assert_eq!(
            request.head_refresh_mode(),
            HeadRefreshMode::UpdateLoadedOnly,
        );

        request.direction = ScanRecordDirectionParam::Desc;
        request.sort = ScanRecordSortParam::Severity;
        assert_eq!(
            request.head_refresh_mode(),
            HeadRefreshMode::UpdateLoadedOnly,
        );
    }

    #[test]
    fn nonterminal_collections_request_bounded_pages_without_cursors() {
        let active = active_records_query();
        assert_eq!(active.collection.as_param(), "active");
        assert!(!active.collection.supports_cursor());
        assert_eq!(active.limit, ACTIVE_PAGE_LIMIT);
        assert!(active.after.is_none());

        let system_id = Uuid::from_u128(7);
        let history = system_history_query(system_id, true);
        assert_eq!(history.collection.as_param(), "history");
        assert!(!history.collection.supports_cursor());
        assert_eq!(history.system_id, Some(system_id));
        assert_eq!(history.limit, SYSTEM_HISTORY_LIMIT);
        assert!(history.after.is_none());
    }

    #[test]
    fn completed_status_and_revision_options_stay_inside_the_server_contract() {
        assert_eq!(
            completed_status_from_value("failed"),
            ScanRecordStatusParam::Failed,
        );
        assert_eq!(
            completed_status_from_value("in_progress"),
            ScanRecordStatusParam::All,
            "an active status is not a terminal filter the server accepts",
        );
        assert_eq!(
            completed_revision_from_value("superseded"),
            ScanRecordRevisionParam::Superseded,
        );
        assert_eq!(
            completed_revision_from_value("unknown"),
            ScanRecordRevisionParam::All,
        );
        assert_eq!(
            default_sort_direction(ScanRecordSortParam::Timestamp),
            ScanRecordDirectionParam::Desc,
            "Completed history reads newest first",
        );
        assert_eq!(
            default_sort_direction(ScanRecordSortParam::Configuration),
            ScanRecordDirectionParam::Asc,
        );
    }
}
