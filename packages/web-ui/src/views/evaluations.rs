//! Displays evaluation queues, history, and persisted evaluation evidence.

mod drawer_header;
mod drawer_snapshot;
mod evidence_navigation;
mod policy_adapter;
mod policy_captured;
mod policy_cells;
mod policy_matrix;
#[cfg(test)]
mod policy_sort_tests;

use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use gloo_timers::future::TimeoutFuture;

use crate::alerts::{
    NAV_BADGES, acknowledge_with_cursor_and_ids_async, attention_row_class, dismiss_attention_item,
    occurrence_id_for_subject, occurrence_ids_for_rendered_subjects, should_flash,
};

use crate::api::{
    client::{
        cancel_commit_evaluation, fetch_eval_dependency_graph, fetch_eval_history,
        fetch_eval_policy_matrix, fetch_eval_queue, force_cancel_commit_evaluation,
        re_evaluate_commit, reorder_eval_queue,
    },
    models::{EvalHistoryItem, EvalHistoryPage, EvalQueueItem, EvalQueueSummary},
};
use crate::components::dialog_focus::{
    DialogFocusBoundary, DialogFocusRestore, DialogFocusSentinel, DialogInitialFocus,
};
use crate::components::{Icon, IconName};
use crate::hooks::{InfiniteScroll, use_infinite_scroll};
use crate::routes::Route;
use crate::state::evaluation_drawer::EvaluationTarget as EvalDrawerTarget;
use crate::state::evaluation_drawer::{
    self as handoff, EVALUATION_RETURN, EvaluationIdentity, EvaluationReturn,
};
use crate::state::navigation_focus::{FocusTarget, NavigationFocus};
use crate::views::latest_filter::{
    LatestFilterState, marker_matches, replace_unique_by, reset_key, retain_visible,
};

const FETCH_LIMIT_MAX: i64 = 10_000; // must match backend LIMIT_MAX

/// Shows the drawer's configuration-level policy counters in design typography.
///
/// The values count evaluated configurations, not bundle controls. Unavailable
/// counters render an em dash rather than a clean zero. Empty known coverage
/// explicitly says `No results`; zero failures alone do not prove a clean gate.
#[component]
fn DrawerPolicyCounts(
    available: bool,
    passed: i64,
    failed: i64,
    #[props(default)] pending: i64,
    #[props(default)] unavailable: i64,
    #[props(default)] evaluation_errors: i64,
    #[props(default)] excluded: i64,
    #[props(default)] live: bool,
) -> Element {
    rsx! {
        if available {
            div { class: "ed-stat-val ed-policy-val",
                title: "Configurations passing / failing the CF-agent and assigned-requirement gate for this attempt. Coverage may be partial; bundle controls are counted in the Policy matrix.",
                span { class: if passed > 0 { "ed-policy-pass" } else { "ed-policy-zero" }, "{passed}" }
                span { class: "ed-policy-sep", "/" }
                span { class: if failed > 0 { "ed-policy-fail" } else { "ed-policy-zero" }, "{failed}" }
                if pending > 0 { span { class: "ed-stat-unit", if live { "{pending} pending" } else { "{pending} unavailable" } } }
                if unavailable > 0 { span { class: "ed-stat-unit", "{unavailable} unavailable" } }
                if evaluation_errors > 0 { span { class: "ed-stat-unit", "{evaluation_errors} eval errors" } }
                if excluded > 0 { span { class: "ed-stat-unit", "{excluded} not selected" } }
                if unavailable == 0 && evaluation_errors == 0 && excluded == 0 && policy_has_no_results(available, passed, failed, pending) { span { class: "ed-stat-unit", "No results" } }
            }
        } else {
            div { class: "ed-stat-val ed-policy-val ed-policy-unavailable",
                title: "Trustworthy attempt-scoped policy counts are unavailable.",
                "—"
                span { class: "ed-stat-unit", if live { " Pending" } else { " Unavailable" } }
            }
        }
    }
}

fn policy_has_no_results(available: bool, passed: i64, failed: i64, pending: i64) -> bool {
    available && passed == 0 && failed == 0 && pending == 0
}

fn format_inventory_count(available: bool, count: i64) -> String {
    if available {
        count.to_string()
    } else {
        "—".into()
    }
}

fn snapshot_population(
    snapshot: &crate::api::models::eval_policy_matrix::EvalPolicyMatrixSnapshot,
) -> i64 {
    let Some(inventory) = &snapshot.inventory else {
        return 0;
    };
    snapshot
        .configuration_summary
        .as_ref()
        .map(|summary| summary.total as i64)
        .unwrap_or_else(|| {
            inventory
                .configurations
                .iter()
                .map(String::as_str)
                .chain(
                    snapshot
                        .rows
                        .iter()
                        .map(|row| row.configuration_name.as_str()),
                )
                .collect::<std::collections::HashSet<_>>()
                .len() as i64
        })
}

/// Refreshes a saved header only from its validated exact-attempt response.
///
/// Retains original flake identity and presentation. Missing attempt data clears
/// old counters and completion metadata; latest queue/history data is never read.
/// Queue DTOs have no actual start/completion fields, so queue time stays queue
/// time. Retained responses cannot establish a derivation count.
fn refresh_retained_return(
    mut saved: EvaluationReturn,
    data: &crate::api::models::EvalPolicyMatrixResponse,
) -> Option<EvaluationReturn> {
    if !saved.identity.matches_response(data) {
        return None;
    }
    let snapshot = data.matrix.as_ref()?;
    let inventory = snapshot.inventory.as_ref();
    let summary = inventory.and(snapshot.configuration_summary.as_ref());
    let total = snapshot_population(snapshot);
    let passed = summary.map_or(0, |summary| summary.passed) as i64;
    let failed = summary.map_or(0, |summary| summary.failed) as i64;
    let evaluation_failed = summary.map_or(0, |summary| summary.evaluation_failed) as i64;
    let pending = summary.map_or(0, |summary| summary.pending) as i64;
    let excluded = summary.map_or(0, |summary| summary.excluded) as i64;
    let unavailable = summary.map_or(0, |summary| summary.unavailable) as i64;
    let status = snapshot
        .attempt_status
        .clone()
        .unwrap_or_else(|| "unavailable".into());
    match saved.target.as_mut() {
        Some(EvalDrawerTarget::Queue(item)) => {
            item.inventory_available = inventory.is_some();
            item.system_count = total;
            item.systems = inventory.map_or_else(Vec::new, |inventory| {
                inventory
                    .configurations
                    .iter()
                    .cloned()
                    .chain(
                        snapshot
                            .rows
                            .iter()
                            .map(|row| row.configuration_name.clone()),
                    )
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect()
            });
            item.policy_counts_available = summary.is_some();
            item.passed_count = passed;
            item.policy_failed_count = failed;
            item.eval_failed_count = evaluation_failed;
            item.policy_pending_count = pending;
            item.unselected_count = excluded;
            item.policy_unavailable_count = unavailable;
            item.derivation_count = None;
            item.evaluation_status = status;
        }
        Some(EvalDrawerTarget::History(item)) => {
            item.inventory_available = inventory.is_some();
            item.system_count = total;
            item.policy_counts_available = summary.is_some();
            item.passed_count = passed;
            item.policy_failed_count = failed;
            item.eval_failed_count = evaluation_failed;
            item.policy_pending_count = pending;
            item.unselected_count = excluded;
            item.policy_unavailable_count = unavailable;
            item.derivation_count = None;
            item.evaluation_status = status;
            item.evaluation_completed_at = snapshot.completed_at;
            item.evaluation_duration_ms = snapshot
                .started_at
                .zip(snapshot.completed_at)
                .map(|(start, end)| (end - start).num_milliseconds())
                .filter(|duration| *duration >= 0);
            // The pinned response does not supply the old summary's error text.
            item.evaluation_error_message = None;
        }
        None => {}
    }
    Some(saved)
}

#[cfg(test)]
mod retained_header_tests {
    use super::*;
    use serde_json::{Value, json};

    fn response() -> crate::api::models::EvalPolicyMatrixResponse {
        serde_json::from_value(json!({
            "commit_id": 7, "policies": [], "systems": [],
            "matrix": {
                "evaluation_attempt_id": uuid::Uuid::from_u128(8),
                "attempt_number": 2, "commit_sha": "a".repeat(40),
                "attempt_status": "failed", "started_at": "2026-10-16T00:00:00Z",
                "completed_at": "2026-10-16T00:01:03Z",
                "provenance": "captured", "columns": [], "bundles": [],
                "empty_bundle_metadata_complete": true, "rows": [],
                "inventory": { "execution_mode": "native",
                    "configurations": ["one", "two", "three", "four", "five"],
                    "selected_configurations": ["one", "two", "three", "four", "five"] },
                "configuration_summary": { "total": 5, "passed": 1, "failed": 1,
                    "evaluation_failed": 1, "pending": 0, "unavailable": 2, "excluded": 0 }
            }
        }))
        .unwrap()
    }

    fn saved(history: bool) -> EvaluationReturn {
        // Deliberately stale/latest-looking fields must all be replaced, while
        // original flake metadata remains useful for a rich return header.
        let item = json!({
            "commit_id": 7, "flake_id": 3, "flake_name": "original", "branch": "main",
            "commit_hash": "a".repeat(40), "commit_message": null, "author": null,
            "committed_at": "2026-10-15T00:00:00Z", "enqueued_at": "2026-10-15T00:00:01Z",
            "evaluation_status": "complete", "queue_position": 1, "systems": ["stale"],
            "system_count": 999, "passed_count": 998, "policy_failed_count": 997,
            "eval_failed_count": 996, "policy_pending_count": 995,
            "policy_unavailable_count": 993, "unselected_count": 992,
            "inventory_available": true, "policy_counts_available": true,
            "derivation_count": 994, "attempt_number": 2,
            "evaluation_completed_at": "2026-10-15T01:00:00Z",
            "evaluation_duration_ms": 999999, "evaluation_error_message": "stale error",
            "alert_occurrence_id": "original"
        });
        let mut saved = EvaluationReturn::direct(EvaluationIdentity {
            commit_id: 7,
            commit_sha: "a".repeat(40),
            attempt_id: uuid::Uuid::from_u128(8),
            attempt_number: 2,
        });
        saved.target = Some(if history {
            EvalDrawerTarget::History(serde_json::from_value(item).unwrap())
        } else {
            EvalDrawerTarget::Queue(serde_json::from_value(item).unwrap())
        });
        saved
    }

    fn target_json(saved: EvaluationReturn) -> Value {
        match saved.target.unwrap() {
            EvalDrawerTarget::Queue(item) => serde_json::to_value(item).unwrap(),
            EvalDrawerTarget::History(item) => serde_json::to_value(item).unwrap(),
        }
    }

    #[test]
    fn retained_headers_replace_stale_counts_with_exact_inventory_not_matrix_rows() {
        let data = response();
        for history in [false, true] {
            let item = target_json(refresh_retained_return(saved(history), &data).unwrap());
            for (key, expected) in [
                ("system_count", 5),
                ("passed_count", 1),
                ("policy_failed_count", 1),
                ("eval_failed_count", 1),
                ("policy_pending_count", 0),
                ("policy_unavailable_count", 2),
                ("unselected_count", 0),
            ] {
                assert_eq!(item[key], json!(expected), "{key}");
            }
            assert_eq!(item["inventory_available"], true);
            assert_eq!(item["policy_counts_available"], true);
            assert_eq!(item["derivation_count"], Value::Null);
            assert_eq!(item["evaluation_status"], "failed");
            assert_eq!(item["flake_name"], "original");
            if history {
                assert_eq!(item["evaluation_completed_at"], "2026-10-16T00:01:03Z");
                assert_eq!(item["evaluation_duration_ms"], 63000);
                assert_eq!(item["evaluation_error_message"], Value::Null);
            } else {
                assert_eq!(
                    item["systems"],
                    json!(["five", "four", "one", "three", "two"])
                );
                assert_eq!(item["enqueued_at"], "2026-10-15T00:00:01Z");
            }
        }
    }

    #[test]
    fn missing_retained_inventory_and_metadata_clear_old_header_fields() {
        let mut data = response();
        let snapshot = data.matrix.as_mut().unwrap();
        snapshot.inventory = None;
        snapshot.configuration_summary = None;
        snapshot.started_at = None;
        snapshot.completed_at = None;
        snapshot.attempt_status = None;
        for history in [false, true] {
            let item = target_json(refresh_retained_return(saved(history), &data).unwrap());
            for key in [
                "system_count",
                "passed_count",
                "policy_failed_count",
                "eval_failed_count",
                "policy_pending_count",
                "policy_unavailable_count",
                "unselected_count",
            ] {
                assert_eq!(item[key], 0, "{key}");
            }
            assert_eq!(item["inventory_available"], false);
            assert_eq!(item["policy_counts_available"], false);
            assert_eq!(item["derivation_count"], Value::Null);
            assert_eq!(item["evaluation_status"], "unavailable");
            if history {
                assert_eq!(item["evaluation_completed_at"], Value::Null);
                assert_eq!(item["evaluation_duration_ms"], Value::Null);
            } else {
                assert_eq!(item["systems"], json!([]));
            }
        }
    }

    #[test]
    fn retained_pending_and_empty_coverage_do_not_claim_clean_results() {
        assert!(policy_has_no_results(true, 0, 0, 0));
        assert!(!policy_has_no_results(false, 0, 0, 0));
        assert!(!policy_has_no_results(true, 0, 0, 3));
        assert!(!policy_has_no_results(true, 1, 0, 0));
        let mut data = response();
        let snapshot = data.matrix.as_mut().unwrap();
        snapshot.configuration_summary = Some(Default::default());
        let inventory = snapshot.inventory.as_mut().unwrap();
        inventory.configurations.clear();
        inventory.selected_configurations.clear();
        let item = target_json(refresh_retained_return(saved(true), &data).unwrap());
        assert_eq!(item["policy_counts_available"], true);
        assert_eq!(item["passed_count"], 0);
        assert_eq!(item["policy_failed_count"], 0);
        assert_eq!(item["policy_pending_count"], 0);
        let snapshot = data.matrix.as_mut().unwrap();
        let summary = snapshot.configuration_summary.as_mut().unwrap();
        summary.total = 3;
        summary.pending = 3;
        let inventory = snapshot.inventory.as_mut().unwrap();
        inventory.configurations = vec!["one".into(), "three".into(), "two".into()];
        inventory.selected_configurations = inventory.configurations.clone();
        snapshot.completed_at = None;
        snapshot.attempt_status = Some("in_progress".into());
        let item = target_json(refresh_retained_return(saved(true), &data).unwrap());
        assert_eq!(item["policy_pending_count"], 3);
        assert_eq!(item["evaluation_status"], "in_progress");
        assert_eq!(item["evaluation_duration_ms"], Value::Null);
    }

    #[test]
    fn retained_refresh_rejects_another_attempt_without_a_latest_fallback() {
        let mut data = response();
        data.matrix.as_mut().unwrap().evaluation_attempt_id = Some(uuid::Uuid::from_u128(9));
        assert!(refresh_retained_return(saved(true), &data).is_none());
        assert!(refresh_retained_return(saved(false), &data).is_none());
    }

    #[test]
    fn retained_policy_coverage_requires_verified_inventory_and_summary() {
        let mut data = response();
        data.matrix.as_mut().unwrap().inventory = None;
        let item = target_json(refresh_retained_return(saved(true), &data).unwrap());
        assert_eq!(item["inventory_available"], false);
        assert_eq!(item["policy_counts_available"], false);
        assert_eq!(item["policy_pending_count"], 0);
        assert_eq!(item["policy_unavailable_count"], 0);

        let mut data = response();
        data.matrix.as_mut().unwrap().configuration_summary = None;
        let item = target_json(refresh_retained_return(saved(false), &data).unwrap());
        assert_eq!(item["inventory_available"], true);
        assert_eq!(item["system_count"], 5);
        assert_eq!(item["policy_counts_available"], false);
        assert_eq!(item["passed_count"], 0);
    }
}

/// Displays the reconciled planned/discovered population after verified discovery.
/// Until discovery exists, no cache-derived number is presented as coverage.
#[component]
fn InventoryCount(available: bool, count: i64, live: bool) -> Element {
    rsx! {
        div { class: "ed-stat-val",
            if available { "{count}" }
            else { "—" span { class: "ed-stat-unit", if live { " Discovering" } else { " Unavailable" } } }
        }
    }
}

/// Shows known attempt-scoped counts without claiming full policy coverage.
#[component]
fn PolicyCounts(available: bool, passed: i64, failed: i64) -> Element {
    rsx! {
        if available {
            span { title: "Known attempt-scoped results; coverage may be partial.",
                span { class: "chip chip-healthy", "{passed} ✓" }
                span { class: if failed > 0 { "chip chip-critical" } else { "chip" }, "{failed} ✗" }
            }
        } else {
            span { class: "chip", title: "Trustworthy attempt-scoped policy counts are unavailable.", "Unavailable" }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EvaluationsTab {
    ActiveQueue,
    History,
}

fn queue_item_matches_search(item: &EvalQueueItem, search: &str) -> bool {
    search.is_empty()
        || [
            item.flake_name.as_str(),
            item.branch.as_str(),
            item.commit_hash.as_str(),
            item.commit_message.as_deref().unwrap_or_default(),
            item.author.as_deref().unwrap_or_default(),
            item.evaluation_status.as_str(),
        ]
        .into_iter()
        .chain(item.systems.iter().map(String::as_str))
        .any(|value| value.to_lowercase().contains(search))
}

/// Includes every effective server criterion and refresh generation.
/// Delimiters in user text cannot alias another request's identity.
type ListRequestKey = (String, String, String, bool, i64, u64);

/// Owns response identity and success/error together, never in separate signals.
type AcceptedList<T> = Option<(ListRequestKey, Result<T, String>)>;

/// Allows an accepted window to remain mounted during same-scope polling or
/// limit growth. A different search/status/flake/latest scope cannot reuse it.
/// Errors replace successes and remain visible until a newer success arrives.
fn cached_list_result<'a, T>(
    stored: &'a AcceptedList<T>,
    current: &ListRequestKey,
) -> Option<&'a Result<T, String>> {
    stored
        .as_ref()
        .filter(|(key, _)| {
            key.0 == current.0 && key.1 == current.1 && key.2 == current.2 && key.3 == current.3
        })
        .map(|(_, result)| result)
}

/// Publishes only the latest full request, including its limit and generation.
/// A late A response cannot overwrite B, including an A→B→A round trip.
fn publish_list<T>(
    stored: &mut AcceptedList<T>,
    current: &ListRequestKey,
    response_key: ListRequestKey,
    result: Result<T, String>,
) {
    if &response_key == current {
        *stored = Some((response_key, result));
    }
}

/// Grants selection pruning only to a success owned by the current full request.
/// Cached windows and loading/error gaps must never remove explicit choices.
fn owned_list_page<'a, T>(stored: &'a AcceptedList<T>, current: &ListRequestKey) -> Option<&'a T> {
    stored
        .as_ref()
        .filter(|(key, _)| key == current)
        .and_then(|(_, result)| result.as_ref().ok())
}

/// Uses the same deduplicated server window for history display and pruning.
fn history_page_rows(page: &EvalHistoryPage, count: usize) -> Vec<EvalHistoryItem> {
    replace_unique_by(page.items.clone(), |item| item.commit_id)
        .into_iter()
        .take(count)
        .collect()
}

fn history_request_key(
    search: &str,
    status: &str,
    flake: &str,
    latest: bool,
    limit: i64,
    generation: u64,
) -> ListRequestKey {
    let filter = |value: &str| match value.trim() {
        "" | "all" => String::new(),
        value => value.to_owned(),
    };
    (
        search.trim().to_owned(),
        filter(status),
        filter(flake),
        latest,
        limit,
        generation,
    )
}

/// Leaves list shortcuts to the focused control, including editable ancestors.
fn list_shortcuts_blocked() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window().and_then(|window| window.document())
            .and_then(|document| document.active_element())
            .is_some_and(|element| element.closest("input, select, textarea, button, a, [contenteditable]:not([contenteditable='false']), [role='button'], [role='checkbox'], [role='combobox'], [role='textbox']").ok().flatten().is_some())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// Keeps row identity stable across paging and ordering changes.
fn evaluation_row_id(history: bool, commit_id: i32) -> String {
    let list = if history { "history" } else { "queue" };
    format!("evaluation-{list}-row-{commit_id}")
}

/// Moves keyboard focus to a rendered row, also making it a dialog opener.
fn focus_evaluation_row(history: bool, commit_id: i32) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        if let Some(row) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(&evaluation_row_id(history, commit_id)))
            .and_then(|row| row.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = row.focus();
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (history, commit_id);
}

#[cfg(test)]
mod request_key_tests {
    use super::{
        AcceptedList, cached_list_result, history_request_key, owned_list_page, publish_list,
    };
    use crate::views::latest_filter::retain_visible;
    use std::collections::HashSet;

    #[test]
    fn same_scope_cache_survives_polling_and_limit_growth_but_not_filter_changes() {
        let key = history_request_key("host", "all", "all", false, 50, 1);
        let stored: AcceptedList<Vec<i32>> = Some((key.clone(), Ok(vec![7])));
        let growing = history_request_key("host", "all", "all", false, 100, 2);
        assert_eq!(cached_list_result(&stored, &growing), Some(&Ok(vec![7])));
        assert!(owned_list_page(&stored, &growing).is_none());
        for different_scope in [
            history_request_key("other", "all", "all", false, 50, 2),
            history_request_key("host", "failed", "all", false, 50, 2),
            history_request_key("host", "all", "flake", false, 50, 2),
            history_request_key("host", "all", "all", true, 50, 2),
        ] {
            assert!(cached_list_result(&stored, &different_scope).is_none());
        }
    }

    #[test]
    fn publication_rejects_reverse_responses_and_superseded_a_to_b_to_a_generations() {
        let a = history_request_key("a", "all", "all", false, 50, 1);
        let b = history_request_key("b", "all", "all", false, 50, 2);
        let newer_a = history_request_key("a", "all", "all", false, 50, 3);
        let mut stored = None;
        publish_list(&mut stored, &b, b.clone(), Ok(vec![2]));
        publish_list(&mut stored, &b, a.clone(), Ok(vec![1]));
        assert_eq!(owned_list_page(&stored, &b), Some(&vec![2]));
        publish_list(&mut stored, &newer_a, a, Ok(vec![1]));
        assert!(cached_list_result(&stored, &newer_a).is_none());
        publish_list(&mut stored, &newer_a, newer_a.clone(), Ok(vec![3]));
        publish_list(&mut stored, &newer_a, b, Err("late error".into()));
        assert_eq!(owned_list_page(&stored, &newer_a), Some(&vec![3]));
    }

    #[test]
    fn selection_prunes_only_from_atomic_owned_success_not_loading_or_cached_page() {
        let key = history_request_key("configuration-only-match", "all", "all", false, 50, 2);
        let old = history_request_key("configuration-only-match", "all", "all", false, 50, 1);
        let mut selected = HashSet::from([7, 8]);
        let mut stored: AcceptedList<Vec<i32>> = None;
        let prune = |stored: &AcceptedList<Vec<i32>>, selected: &mut HashSet<i32>| {
            if let Some(ids) = owned_list_page(stored, &key) {
                retain_visible(selected, ids.iter().copied());
            }
        };
        prune(&stored, &mut selected);
        stored = Some((old, Ok(vec![])));
        prune(&stored, &mut selected);
        assert_eq!(selected, HashSet::from([7, 8]));
        publish_list(&mut stored, &key, key.clone(), Ok(vec![7]));
        prune(&stored, &mut selected);
        assert_eq!(selected, HashSet::from([7]));
        selected.clear();
        prune(&stored, &mut selected);
        assert!(selected.is_empty());
    }

    #[test]
    fn accepted_errors_replace_cached_success_without_pruning_selection() {
        let key = history_request_key("host", "all", "all", false, 50, 1);
        let next = history_request_key("host", "all", "all", false, 50, 2);
        let mut stored: AcceptedList<Vec<i32>> = Some((key, Ok(vec![7])));
        publish_list(&mut stored, &next, next.clone(), Err("unavailable".into()));
        assert_eq!(
            cached_list_result(&stored, &next),
            Some(&Err("unavailable".into()))
        );
        assert!(owned_list_page(&stored, &next).is_none());
    }

    #[test]
    fn history_key_normalizes_and_covers_all_server_criteria_and_generation() {
        let key = history_request_key(" host ", "all", "all", false, 50, 1);
        assert_eq!(key, history_request_key("host", "", "", false, 50, 1));
        for other in [
            history_request_key("other", "all", "all", false, 50, 1),
            history_request_key("host", "failed", "all", false, 50, 1),
            history_request_key("host", "all", "flake", false, 50, 1),
            history_request_key("host", "all", "all", true, 50, 1),
            history_request_key("host", "all", "all", false, 100, 1),
            history_request_key("host", "all", "all", false, 50, 3),
        ] {
            assert_ne!(key, other);
        }
    }
}

#[component]
pub fn EvaluationsView() -> Element {
    rsx! { EvaluationsPage {} }
}

#[component]
pub fn EvaluationsCommitView(commit_id: i32) -> Element {
    rsx! { EvaluationsPage { key: "{commit_id}", requested_commit: Some(commit_id) } }
}

/// Opens a retained attempt from its shareable path, including after refresh.
///
/// Missing or unauthorized attempts never fall back to the latest evaluation.
#[component]
pub fn EvaluationsAttemptView(commit_id: i32, attempt_id: uuid::Uuid) -> Element {
    rsx! { EvaluationsPage { key: "{commit_id}-{attempt_id}", requested_commit: Some(commit_id), requested_attempt: Some(attempt_id) } }
}

#[component]
fn EvaluationsPage(
    #[props(default)] requested_commit: Option<i32>,
    #[props(default)] requested_attempt: Option<uuid::Uuid>,
) -> Element {
    let nav = navigator();
    let mut navigation_focus = use_context::<Signal<Option<NavigationFocus>>>();
    let mut stored_queue =
        use_signal(|| None::<(ListRequestKey, Result<EvalQueueSummary, String>)>);
    let mut active_fetch_limit = use_signal(|| 200_i64);
    let mut active_refresh = use_signal(|| 0_u64);
    let mut history_refresh = use_signal(|| 0_u64);
    let mut active_tab = use_signal(|| EvaluationsTab::ActiveQueue);
    let mut latest_filter = use_signal(LatestFilterState::default);
    let mut search_query = use_signal(String::new);
    let queue_generation = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(0_u64)));
    let queue_key = use_memo(move || {
        let _ = active_refresh();
        queue_generation.set(queue_generation.get() + 1);
        history_request_key(
            &search_query(),
            "",
            "",
            latest_filter().enabled(),
            active_fetch_limit(),
            queue_generation.get(),
        )
    });
    let queue_resource =
        use_memo(move || cached_list_result(&stored_queue.read(), &queue_key()).cloned());
    let queue_items = use_memo(move || {
        cached_list_result(&stored_queue.read(), &queue_key())
            .and_then(|result| result.as_ref().ok())
            .map(|summary| replace_unique_by(summary.items.clone(), |item| item.commit_id))
            .unwrap_or_default()
    });
    let mut drawer_target = use_signal(|| None::<EvalDrawerTarget>);
    // Selection is explicit. Refreshes and filters may prune selected rows but
    // must never select new rows or restore a cleared selection.
    let mut history_selected_ids = use_signal(std::collections::HashSet::<i32>::new);
    // Keyboard navigation: index into the currently visible list (queue or history).
    // Keyboard focus is separate from bulk selection.
    let mut focused_index: Signal<Option<usize>> = use_signal(|| None);

    // Active queue multi-select (bulk cancel)
    let mut active_selected_ids = use_signal(std::collections::HashSet::<i32>::new);

    // Toast state for soft-cancel undo
    let mut toast_msg = use_signal(|| None::<String>);

    // History tab state
    let mut history_fetch_limit = use_signal(|| 50_i64);
    let mut history_status_filter = use_signal(|| String::from("all"));
    let mut history_flake_filter = use_signal(|| String::from("all"));
    let mut history_ack_cursor = use_signal(|| None::<String>);
    let mut evals_ack_sent = use_signal(|| false);
    // Accumulated history items across the current page-1 fetch window.
    let mut stored_history =
        use_signal(|| None::<(ListRequestKey, Result<EvalHistoryPage, String>)>);
    let history_generation = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(0_u64)));
    let history_key = use_memo(move || {
        let _ = history_refresh();
        history_generation.set(history_generation.get() + 1);
        history_request_key(
            &search_query(),
            &history_status_filter(),
            &history_flake_filter(),
            latest_filter().enabled(),
            history_fetch_limit(),
            history_generation.get(),
        )
    });
    let history_resource =
        use_memo(move || cached_list_result(&stored_history.read(), &history_key()).cloned());
    let history_items_acc = use_memo(move || {
        cached_list_result(&stored_history.read(), &history_key())
            .and_then(|result| result.as_ref().ok())
            .map(|page| history_page_rows(page, usize::MAX))
            .unwrap_or_default()
    });
    let history_total_acc = use_memo(move || {
        cached_list_result(&stored_history.read(), &history_key())
            .and_then(|result| result.as_ref().ok())
            .map_or(0, |page| page.total_count)
    });
    let history_domain_total = use_memo(move || {
        cached_list_result(&stored_history.read(), &history_key())
            .and_then(|result| result.as_ref().ok())
            .map_or(0, |page| page.domain_total)
    });
    let initial_return = use_hook(move || {
        EVALUATION_RETURN.peek().clone().filter(|r| {
            requested_commit.is_some_and(|commit_id| r.matches_commit(commit_id))
                && requested_attempt.is_none_or(|attempt_id| r.identity.attempt_id == attempt_id)
        })
    });
    let mut restored = use_signal(|| false);
    let mut direct_return = use_signal(|| None::<EvaluationReturn>);
    let return_to_validate = initial_return.clone();
    let return_validation = use_resource(move || {
        let saved = return_to_validate.clone();
        async move {
            let exact = requested_commit.zip(requested_attempt).or_else(|| {
                saved
                    .as_ref()
                    .map(|s| (s.identity.commit_id, s.identity.attempt_id))
            });
            let Some((commit_id, attempt_id)) = exact else {
                return None;
            };
            crate::api::client::fetch_eval_attempt_policy_matrix(commit_id, attempt_id)
                .await
                .ok()
                .and_then(|data| {
                    let identity = EvaluationIdentity::from_response(&data, commit_id, attempt_id)?;
                    if let Some(saved) = saved {
                        // Invalid saved identity must fail closed, even if the
                        // server returned another otherwise valid exact context.
                        refresh_retained_return(saved, &data)
                    } else {
                        Some(EvaluationReturn::direct(identity))
                    }
                })
        }
    });
    let return_for_effect = initial_return.clone();
    use_effect(move || {
        if restored() {
            return;
        }
        if return_for_effect.is_some() || requested_attempt.is_some() {
            // Do not apply saved page or drawer choices to an unvalidated
            // response. Missing/unauthorized historical attempts fail closed.
            let Some(Some(saved)) = return_validation.read().clone() else {
                return;
            };
            active_tab.set(if saved.history {
                EvaluationsTab::History
            } else {
                EvaluationsTab::ActiveQueue
            });
            search_query.set(saved.search.clone());
            history_status_filter.set(saved.status.clone());
            history_flake_filter.set(saved.flake.clone());
            if latest_filter().enabled() != saved.latest {
                latest_filter.write().toggle();
            }
            // Publish only validated identity before mounting a pinned Policy
            // tab. Its initial request must never choose the latest endpoint.
            *EVALUATION_RETURN.write() = Some(saved.clone());
            let Some(target) = saved.target.clone() else {
                direct_return.set(Some(saved));
                restored.set(true);
                return;
            };
            drawer_target.set(Some(target));
            restored.set(true);
        } else if let Some(id) = requested_commit {
            active_fetch_limit.set(FETCH_LIMIT_MAX);
            history_fetch_limit.set(FETCH_LIMIT_MAX);
            if let Some(item) = queue_items
                .read()
                .iter()
                .find(|item| item.commit_id == id)
                .cloned()
            {
                drawer_target.set(Some(EvalDrawerTarget::Queue(item)));
                restored.set(true);
            } else if let Some(item) = history_items_acc
                .read()
                .iter()
                .find(|item| item.commit_id == id)
                .cloned()
            {
                active_tab.set(EvaluationsTab::History);
                drawer_target.set(Some(EvalDrawerTarget::History(item)));
                restored.set(true);
            }
        }
    });

    use_effect(move || {
        let selected = drawer_target();
        let queue = queue_items.read();
        let history = history_items_acc.read();
        if let Some(current) = selected.as_ref() {
            // A return is pinned to the original attempt. Summary polling may
            // discover a retry, but must not retarget historical presentation.
            if EVALUATION_RETURN
                .read()
                .as_ref()
                .is_some_and(|saved| match current {
                    EvalDrawerTarget::Queue(item) => item.commit_id == saved.identity.commit_id,
                    EvalDrawerTarget::History(item) => item.commit_id == saved.identity.commit_id,
                })
            {
                return;
            }
            let updated = drawer_snapshot::refresh(current, &queue, &history);
            // The effect also subscribes to selection changes. Only publish a
            // changed snapshot so its own write cannot sustain a rerender loop.
            if &updated != current {
                drawer_target.set(Some(updated));
            }
        }
    });

    use_effect(move || {
        let _ = (search_query(), latest_filter().enabled());
        active_fetch_limit.set(200);
    });

    use_effect(move || {
        let _ = (
            history_status_filter(),
            history_flake_filter(),
            search_query(),
            latest_filter().enabled(),
        );
        history_fetch_limit.set(50);
    });

    let _history_request = use_resource(move || {
        let key = history_key();
        async move {
            let (search, status, flake, latest_only, limit, _) = &key;
            let result = fetch_eval_history(
                1,
                *limit,
                if status.is_empty() || status == "all" {
                    None
                } else {
                    Some(status.as_str())
                },
                if flake.is_empty() || flake == "all" {
                    None
                } else {
                    Some(flake.as_str())
                },
                (!search.is_empty()).then_some(search.as_str()),
                *latest_only,
            )
            .await;
            let current = history_key.peek().clone();
            if key == current {
                publish_list(
                    &mut stored_history.write(),
                    &current,
                    key,
                    result.map_err(|error| error.to_string()),
                );
                history_ack_cursor.set(NAV_BADGES.read_unchecked().observed_at.clone());
            }
        }
    });

    let _queue_request = use_resource(move || {
        let key = queue_key();
        async move {
            let result =
                fetch_eval_queue(key.4, (!key.0.is_empty()).then_some(key.0.as_str()), key.3).await;
            let current = queue_key.peek().clone();
            if key == current {
                publish_list(
                    &mut stored_queue.write(),
                    &current,
                    key,
                    result.map_err(|error| error.to_string()),
                );
            }
        }
    });

    {
        let mut active_refresh = active_refresh.clone();
        let mut history_refresh = history_refresh.clone();
        let mut active_tab = active_tab;
        use_future(move || async move {
            #[cfg(target_arch = "wasm32")]
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    TimeoutFuture::new(3000).await;
                    if active_tab() == EvaluationsTab::ActiveQueue {
                        active_refresh.set(active_refresh() + 1);
                    } else {
                        history_refresh.set(history_refresh() + 1);
                    }
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            let _ = (active_refresh, history_refresh);
        });
    }

    use_effect(move || {
        let Some(focus) = navigation_focus() else {
            return;
        };
        if focus.target != FocusTarget::Evaluations {
            return;
        }

        active_fetch_limit.set(FETCH_LIMIT_MAX);
        history_fetch_limit.set(FETCH_LIMIT_MAX);

        let queue_snapshot = queue_items.read();
        let active_match = queue_snapshot.iter().find(|item| {
            focus
                .commit_sha
                .as_deref()
                .map(|sha| item.commit_hash == sha)
                .unwrap_or(false)
                && focus
                    .flake_name
                    .as_deref()
                    .map(|flake| item.flake_name == flake)
                    .unwrap_or(true)
        });

        if let Some(item) = active_match.cloned() {
            active_tab.set(EvaluationsTab::ActiveQueue);
            drawer_target.set(Some(EvalDrawerTarget::Queue(item)));
            navigation_focus.set(None);
            return;
        }

        let history_snapshot = history_items_acc.read();
        let history_match = history_snapshot.iter().find(|item| {
            focus
                .commit_sha
                .as_deref()
                .map(|sha| item.commit_hash == sha)
                .unwrap_or(false)
                && focus
                    .flake_name
                    .as_deref()
                    .map(|flake| item.flake_name == flake)
                    .unwrap_or(true)
        });

        if let Some(item) = history_match.cloned() {
            active_tab.set(EvaluationsTab::History);
            drawer_target.set(Some(EvalDrawerTarget::History(item)));
            navigation_focus.set(None);
            return;
        }

        let history_loaded = history_resource
            .read()
            .as_ref()
            .is_some_and(|result| result.is_ok());
        let active_loaded = queue_resource
            .read()
            .as_ref()
            .is_some_and(|result| result.is_ok());
        let history_exhausted =
            history_loaded && history_fetch_limit() >= history_total_acc().min(FETCH_LIMIT_MAX);
        let active_exhausted = active_loaded && active_fetch_limit() >= FETCH_LIMIT_MAX;

        if history_exhausted && active_exhausted {
            navigation_focus.set(None);
        }
    });

    // Infinite-scroll paging for history — created in the parent so the
    // parent knows the visible row count for keyboard navigation (review
    // finding #4) and can own the server-page advancement effect.
    let hist_reset_key = reset_key(
        "hist",
        &[
            history_status_filter().as_str(),
            history_flake_filter().as_str(),
            search_query().trim(),
        ],
        latest_filter().enabled(),
    );
    let hist_paging = use_infinite_scroll(hist_reset_key, 20);

    // Grow the server fetch limit for history, reactive because all signal
    // reads are inside the closure (review finding #1).
    use_effect(move || {
        let loaded_history_len = history_items_acc.read().len();
        let history_total = history_total_acc();
        let requested_len = history_fetch_limit();
        let hist_server_has_more = (loaded_history_len as i64) < history_total.min(FETCH_LIMIT_MAX);
        let hist_count = hist_paging.count();
        if (loaded_history_len as i64) >= requested_len
            && hist_count >= loaded_history_len
            && loaded_history_len > 0
            && hist_server_has_more
        {
            history_fetch_limit.with_mut(|limit| {
                *limit = (*limit + 50).min(FETCH_LIMIT_MAX);
            });
        }
        // Re-evaluate the sentinel after the list may have grown.
        hist_paging.recheck(hist_paging.count().min(loaded_history_len));
    });

    let active_items = queue_items
        .read()
        .iter()
        .filter(|item| is_active_eval_status(&item.evaluation_status))
        .filter(|item| queue_item_matches_search(item, &search_query().trim().to_lowercase()))
        .filter(|item| marker_matches(latest_filter().enabled(), item.is_latest_per_flake))
        .cloned()
        .collect::<Vec<_>>();

    // Infinite-scroll paging for the active queue.
    let active_paging = use_infinite_scroll(
        reset_key(
            "active",
            &[search_query().trim()],
            latest_filter().enabled(),
        ),
        20,
    );
    let paged_active_items: Vec<EvalQueueItem> = active_items
        .iter()
        .take(active_paging.count())
        .cloned()
        .collect();
    let active_total = queue_resource
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .map(|summary| summary.filtered_total)
        .unwrap_or(0);
    let active_has_more = active_paging.count() < active_items.len()
        || (active_items.len() as i64) < active_total.min(FETCH_LIMIT_MAX);

    use_effect(move || {
        if active_tab() == EvaluationsTab::ActiveQueue {
            let loaded_active_len = queue_items
                .read()
                .iter()
                .filter(|item| is_active_eval_status(&item.evaluation_status))
                .count();
            let requested_len = active_fetch_limit();
            let active_total = queue_resource
                .read()
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .map(|summary| summary.filtered_total)
                .unwrap_or(0);
            if (loaded_active_len as i64) >= requested_len
                && active_paging.count() >= loaded_active_len
                && (loaded_active_len as i64) < active_total.min(FETCH_LIMIT_MAX)
            {
                active_fetch_limit.with_mut(|limit| {
                    *limit = (*limit + 200).min(FETCH_LIMIT_MAX);
                });
            }
            // Re-evaluate the sentinel after the list may have grown.
            active_paging.recheck(active_paging.count().min(loaded_active_len));
        }
    });

    let summary_snapshot = queue_resource
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned();

    let active_count = summary_snapshot
        .as_ref()
        .map(|s| s.active_count)
        .unwrap_or(0);
    let active_domain_total = summary_snapshot
        .as_ref()
        .map(|s| s.domain_total)
        .unwrap_or(0);
    let completed_count = summary_snapshot
        .as_ref()
        .map(|s| s.completed_count)
        .unwrap_or(0);
    let failed_count = summary_snapshot
        .as_ref()
        .map(|s| s.failed_count)
        .unwrap_or(0);
    let total_count = active_count + completed_count;
    let history_count = history_resource
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .map(|page| page.total_count)
        .unwrap_or(0);

    use_effect(move || {
        let stored = stored_queue.read();
        let Some(summary) = owned_list_page(&stored, &queue_key()) else {
            return;
        };
        let visible = replace_unique_by(summary.items.clone(), |item| item.commit_id)
            .iter()
            .filter(|item| is_active_eval_status(&item.evaluation_status))
            .filter(|item| queue_item_matches_search(item, &search_query().trim().to_lowercase()))
            .filter(|item| marker_matches(latest_filter().enabled(), item.is_latest_per_flake))
            .take(active_paging.count())
            .map(|item| item.commit_id)
            .collect::<Vec<_>>();
        let mut selected = active_selected_ids.read().clone();
        if retain_visible(&mut selected, visible) {
            active_selected_ids.set(selected);
        }
    });

    // The server searches authorized configuration names absent from this DTO.
    // Its matching page is authoritative for rendering, selection, and keys.
    let history_visible_rows = use_memo(move || {
        cached_list_result(&stored_history.read(), &history_key())
            .and_then(|result| result.as_ref().ok())
            .map(|page| history_page_rows(page, hist_paging.count()))
            .unwrap_or_default()
    });
    use_effect(move || {
        // Read identity and rows from the same publication. A separate derived
        // visible-row memo can still contain its loading value in this effect.
        let stored = stored_history.read();
        let Some(page) = owned_list_page(&stored, &history_key()) else {
            return;
        };
        let visible = history_page_rows(page, hist_paging.count())
            .iter()
            .map(|item| item.commit_id)
            .collect::<Vec<_>>();
        let mut selected = history_selected_ids.read().clone();
        if retain_visible(&mut selected, visible) {
            history_selected_ids.set(selected);
        }
    });
    use_effect(move || {
        if active_tab() == EvaluationsTab::History {
            if let Some(Ok(page_data)) = history_resource.read().as_ref() {
                let unfiltered_first_page =
                    history_status_filter() == "all" && history_flake_filter() == "all";
                // Acknowledge occurrences for whatever is currently rendered,
                // without waiting for the full history to load. Each rendered
                // window acknowledges only the occurrences whose subjects are
                // present in `history_items_acc`, so failures beyond the visible
                // page are not silently consumed. The async acknowledgment has
                // built-in payload deduplication, so redundant calls when the
                // history resource refreshes with the same data are no-ops.
                // As the user scrolls, new rendered subjects produce a different
                // payload key, triggering a new POST for the additional items.
                if unfiltered_first_page && !page_data.items.is_empty() {
                    let Some(cursor) = history_ack_cursor.read().clone() else {
                        return;
                    };
                    // Bound acknowledgment to occurrences for commits actually
                    // present in the loaded/accumulated history, not every
                    // eligible occurrence fleet-wide — a failure beyond the
                    // 10,000-row fetch cap must not be silently consumed.
                    let rendered_commit_ids: std::collections::HashSet<String> = history_items_acc
                        .read()
                        .iter()
                        .map(|item| item.commit_id.to_string())
                        .collect();
                    let occurrence_ids =
                        occurrence_ids_for_rendered_subjects("evals", &rendered_commit_ids);
                    spawn(async move {
                        let _ =
                            acknowledge_with_cursor_and_ids_async("evals", cursor, occurrence_ids)
                                .await;
                    });
                }
            }
        }
    });
    let selected_history_rows = {
        let ids = history_selected_ids.read();
        history_visible_rows
            .read()
            .iter()
            .filter(|item| ids.contains(&item.commit_id))
            .cloned()
            .collect::<Vec<_>>()
    };
    let selected_count = selected_history_rows.len();
    let active_selected_visible_ids = paged_active_items
        .iter()
        .map(|item| item.commit_id)
        .filter(|id| active_selected_ids.read().contains(id))
        .collect::<Vec<_>>();
    let active_selected_visible_count = active_selected_visible_ids.len();
    let same_flake_pair = selected_history_rows.len() == 2
        && selected_history_rows[0].flake_name == selected_history_rows[1].flake_name;
    let compare_disabled = selected_count != 2 || !same_flake_pair;
    let compare_title = if selected_count != 2 {
        "Select exactly 2 evaluations to compare"
    } else if !same_flake_pair {
        "Compare only works for two evaluations of the same flake"
    } else {
        "Compare selected evaluations"
    };

    // Server-computed "new failed evaluations since last acknowledgment"
    // (persists across page refresh/re-login — see alerts::NAV_BADGES).
    // Drives both the History tab's badge/attention-flash-tab pulse and the
    // one-shot row flash below, replacing a raw total that would otherwise
    // reappear identically on every reload. should_flash still guards the
    // one-shot-per-page-load timing (safe: reads/writes ALERT_STATE, not a
    // signal read inside this same effect).
    let evals_failed_new = NAV_BADGES().evals_failed_new;
    let mut flash_evals_signal = use_signal(|| false);
    let flash_evals = flash_evals_signal();
    use_effect(move || {
        if should_flash("evals", evals_failed_new > 0) {
            flash_evals_signal.set(true);
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(3200).await;
                flash_evals_signal.set(false);
            });
        }
    });

    rsx! {
            div {
                style: "display: flex; flex-direction: column; gap: 16px; outline: none;",
                tabindex: 0,
                onkeydown: move |evt| {
                    if list_shortcuts_blocked() { return; }
                    // No keyboard nav while drawer is open
                    if direct_return.peek().is_some() {
                        if evt.key() == Key::Escape {
                            direct_return.set(None);
                            *EVALUATION_RETURN.write() = None;
                            nav.push(Route::EvaluationsView {});
                        }
                        return;
                    }
                    if drawer_target.read().is_some() {
                        if evt.key() == Key::Escape {
                            drawer_target.set(None);
                            *EVALUATION_RETURN.write() = None;
                        }
                        return;
                    }

                    // Bounds and DOM targets both come from the rendered slice.
                    let list_len = if active_tab() == EvaluationsTab::ActiveQueue {
                        paged_active_items.len()
                    } else {
                        history_visible_rows.read().len()
                    };

                    let direction = match evt.key() {
                        Key::ArrowDown => Some(true),
                        Key::ArrowUp => Some(false),
                        Key::Character(ref c) if c == "j" => Some(true),
                        Key::Character(ref c) if c == "k" => Some(false),
                        _ => None,
                    };
                    if let Some(forward) = direction {
                        evt.prevent_default();
                        if list_len == 0 { return; }
                        let next = match focused_index() {
                            None => 0,
                            Some(i) if forward => i.saturating_add(1).min(list_len - 1),
                            Some(i) => i.saturating_sub(1).min(list_len - 1),
                        };
                        let history = active_tab() == EvaluationsTab::History;
                        let commit_id = if history {
                            history_visible_rows.read().get(next).map(|item| item.commit_id)
                        } else {
                            paged_active_items.get(next).map(|item| item.commit_id)
                        };
                        if let Some(commit_id) = commit_id {
                            focused_index.set(Some(next));
                            focus_evaluation_row(history, commit_id);
                        }
                        return;
                    }
                    match evt.key() {
                        Key::Enter => {
                            if let Some(idx) = focused_index() {
                                if active_tab() == EvaluationsTab::ActiveQueue {
                                    // Use paged slice — idx is clamped to its length above.
                                    if let Some(ev) = paged_active_items.get(idx) {
                                        drawer_target.set(Some(EvalDrawerTarget::Queue(ev.clone())));
                                    }
                                } else {
                                    // Use the same visible slice as row rendering.
                                    let item = history_visible_rows.read().get(idx).cloned();
                                    if let Some(ev) = item {
                                        drawer_target.set(Some(EvalDrawerTarget::History(ev)));
                                    }
                                }
                            }
                        }
                        Key::Character(ref c) if c == "c" => {
                            if active_tab() == EvaluationsTab::ActiveQueue {
                                if let Some(idx) = focused_index() {
                                    if let Some(ev) = paged_active_items.get(idx) {
                                        let commit_id = ev.commit_id;
                                        let can_cancel = matches!(
                                            ev.evaluation_status.as_str(),
                                            "pending" | "in_progress"
                                        );
                                        if can_cancel {
                                            let mut refresh_sig = active_refresh;
                                            let mut toast = toast_msg;
                                            spawn(async move {
                                                if let Err(e) = cancel_commit_evaluation(commit_id).await {
                                                    toast.set(Some(format!("Cancel failed: {}", e)));
                                                } else {
                                                    refresh_sig.set(refresh_sig() + 1);
                                                }
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        Key::Escape => {
                            focused_index.set(None);
                        }
                        _ => {}
                    }
                },

                // Page head (matching JSX: title, subtitle, LiveIndicator)
                div {
                    class: "page-head",
                    div {
                        h1 { class: "page-title", "Evaluations" }
                        p {
                            class: "page-subtitle",
                            "{active_count} active · {completed_count} completed · {failed_count} failed"
                        }
                    }
                    div {
                        style: "display: flex; gap: 12px; align-items: center;",
                        LiveIndicator {}
                    }
                }

                // Stat strip
                if requested_attempt.is_some() && return_validation.read().is_none() {
                    p { role: "status", "Loading exact retained attempt…" }
                }
                if (requested_attempt.is_some() || initial_return.is_some()) && matches!(&*return_validation.read(), Some(None)) {
                    div { role: "alert", "Exact evaluation attempt unavailable. Saved drawer state was not applied to another attempt." }
                    button { class: "btn btn-ghost focus-ring", onclick: move |_| { *EVALUATION_RETURN.write() = None; nav.push(Route::EvaluationsView {}); }, "Open latest evaluations" }
                }
                div {
                    class: "stat-strip",
                    div {
                        class: "stat",
                        span {
                            class: "stat-accent",
                            style: "--stat-color: #60a5fa;"
                        }
                        div { class: "stat-label", "Active" }
                        div {
                            class: "stat-value",
                            style: "color: #60a5fa;",
                            "{active_count}"
                        }
                    }
                    div {
                        class: "stat",
                        span {
                            class: "stat-accent",
                            style: "--stat-color: #34d399;"
                        }
                        div { class: "stat-label", "Completed" }
                        div {
                            class: "stat-value",
                            style: "color: #34d399;",
                            "{completed_count}"
                        }
                    }
                    div {
                        class: "stat",
                        span {
                            class: "stat-accent",
                            style: "--stat-color: #f87171;"
                        }
                        div { class: "stat-label", "Failed" }
                        div {
                            class: "stat-value",
                            style: "color: #f87171;",
                            "{failed_count}"
                        }
                    }
                    div {
                        class: "stat",
                        span {
                            class: "stat-accent",
                            style: "--stat-color: var(--cf-text-secondary);"
                        }
                        div { class: "stat-label", "Total" }
                        div { class: "stat-value", "{total_count}" }
                    }
                }

                // Tabs
                div {
                    class: "card",
                    style: "overflow: hidden;",

                    div {
                        class: "sd-tabs q-tabbar",
                        style: "padding: 0 16px; border-bottom: 1px solid var(--cf-card-border);",
                        button {
                            class: if active_tab() == EvaluationsTab::ActiveQueue {
                                "sd-tab focus-ring active"
                            } else {
                                "sd-tab focus-ring"
                            },
                            onclick: move |_| { active_tab.set(EvaluationsTab::ActiveQueue); focused_index.set(None); },
                            "Active Queue"
                            span { class: "sd-tab-badge", "{active_count}" }
                        }
                        button {
                            class: if active_tab() == EvaluationsTab::History {
                                "sd-tab focus-ring active"
                            } else if evals_failed_new > 0 {
                                "sd-tab focus-ring attention-flash-tab"
                            } else {
                                "sd-tab focus-ring"
                            },
                            onclick: move |_| {
                                active_tab.set(EvaluationsTab::History);
                                focused_index.set(None);
                                // Acknowledge the "evals" sidebar/tab badge when History tab
                                // is opened (persists server-side — TASK-385 follow-up).
                                evals_ack_sent.set(false);
                            },
                            "Completed"
                            span { class: "sd-tab-badge", "{history_count}" }
                        }
                        div { class: "q-tabbar-actions",
                            // Show a Shift-click hint only on History where Shift-click
                            // actually toggles row selection.
                            if active_tab() == EvaluationsTab::History && history_total_acc() > 0 {
                                span {
                                    class: "ms-hint",
                                    title: "Shift-click to toggle row selection",
                                    kbd { "⇧" }
                                    "-click to select"
                                }
                            }
                            button {
                                class: if latest_filter().enabled() {
                                    "btn btn-ghost xs focus-ring active-filter"
                                } else {
                                    "btn btn-ghost xs focus-ring"
                                },
                                title: "Show only the most recent evaluation per flake",
                                aria_pressed: latest_filter().enabled(),
                                onclick: move |_| latest_filter.with_mut(LatestFilterState::toggle),
                                Icon { name: IconName::Star, size: 12 }
                                "Latest per flake"
                            }
                            div {
                                class: "q-search",
                            Icon { name: IconName::Search, size: 13 }
                            input {
                                class: "q-search-input",
                                placeholder: if active_tab() == EvaluationsTab::ActiveQueue { "Search queue…" } else { "Search completed…" },
                                value: "{search_query}",
                                oninput: move |event| search_query.set(event.value()),
                            }
                            if !search_query.read().trim().is_empty() {
                                span {
                                    class: "q-search-count",
                                    if active_tab() == EvaluationsTab::ActiveQueue {
                                        "{active_count} of {active_domain_total}"
                                    } else {
                                        "{history_count} of {history_domain_total()}"
                                    }
                                }
                                button {
                                    class: "btn-icon xs focus-ring",
                                    title: "Clear search",
                                    onclick: move |_| search_query.set(String::new()),
                                    Icon { name: IconName::X, size: 13 }
                                }
                            }
                        }
                    }
                    }

                    if active_tab() == EvaluationsTab::ActiveQueue {
                        if active_domain_total == 0 {
                            div { class: "q-empty",
                                h3 { "No active evaluations" }
                                div { "All flake evaluations are complete." }
                            }
                        } else if active_items.is_empty() {
                            div { class: "q-empty",
                                Icon { name: IconName::Search, size: 20 }
                                h3 { "No matching evaluations" }
                                div { "Try adjusting your search or filters." }
                                button {
                                    class: "btn btn-ghost xs focus-ring",
                                    onclick: move |_| {
                                        search_query.set(String::new());
                                        latest_filter.with_mut(LatestFilterState::clear);
                                    },
                                    "Clear active filters"
                                }
                            }
                        } else {
                            EvalActiveQueue {
                                evals: paged_active_items.clone(),
                                refresh: active_refresh,
                                queue_items: queue_items,
                                drawer_target: drawer_target,
                                focused_index: focused_index,
                                active_selected_ids: active_selected_ids,
                                toast_msg: toast_msg,
                                allow_reorder: !latest_filter().enabled() && search_query.read().trim().is_empty(),
                            }
                        }
                        if active_has_more {
                            div {
                                class: "infinite-sentinel",
                                "data-sentinel": active_paging.sentinel_id(),
                                onmounted: move |_| active_paging.check_and_register(),
                                "Loading more…"
                            }
                        }
                    }

                    if active_tab() == EvaluationsTab::History {
                        if selected_count > 0 {
                            div {
                                class: "ed-bulkbar",
                                span {
                                    style: "font-size: 13px; font-weight: 600;",
                                    "{selected_count} selected"
                                }
                                div { style: "flex: 1;" }
                                button {
                                    class: "btn btn-ghost focus-ring xs",
                                    onclick: move |_| {
                                        let selected_ids = selected_history_rows.iter().map(|item| item.commit_id).collect::<Vec<_>>();
                                        let mut refresh_sig = history_refresh.clone();
                                        let mut selected_sig = history_selected_ids.clone();
                                        let mut toast = toast_msg.clone();
                                        spawn(async move {
                                            let mut success = 0u32;
                                            let mut failed: Vec<i32> = Vec::new();
                                            for commit_id in selected_ids {
                                                match re_evaluate_commit(commit_id).await {
                                                    Ok(_) => success += 1,
                                                    Err(_) => failed.push(commit_id),
                                                }
                                            }
                                            if success > 0 {
                                                toast.set(Some(format!("Re-queued {} evaluation{}", success, if success == 1 { "" } else { "s" })));
                                                selected_sig.set(failed.into_iter().collect());
                                            } else {
                                                toast.set(Some("Re-evaluate failed — see server logs".to_string()));
                                            }
                                            refresh_sig.set(refresh_sig() + 1);
                                        });
                                    },
                                    Icon { name: IconName::Sync, size: 11 }
                                    " Re-evaluate"
                                }
                                button {
                                    class: "btn btn-ghost focus-ring xs",
                                    disabled: compare_disabled,
                                    title: "{compare_title}",
                                    style: if compare_disabled {
                                        "opacity: 0.4; cursor: not-allowed;"
                                    } else {
                                        ""
                                    },
                                    "Compare"
                                }
                                button {
                                    class: "btn btn-ghost focus-ring xs",
                                    Icon { name: IconName::Download, size: 11 }
                                    " Download logs"
                                }
                                 button {
                                    class: "btn-icon focus-ring",
                                    onclick: move |_| {
                                        history_selected_ids.write().clear();
                                    },
                                    title: "Clear",
                                    Icon { name: IconName::X, size: 14 }
                                }
                            }
                        }

                        EvalHistory {
                            visible_rows: history_visible_rows,
                            history_resource: history_resource,
                            history_status_filter: history_status_filter,
                            history_flake_filter: history_flake_filter,
                            history_fetch_limit: history_fetch_limit,
                            refresh: history_refresh,
                            history_selected_ids: history_selected_ids,
                            drawer_target: drawer_target,
                            focused_index: focused_index,
                            flash_evals: flash_evals,
                            history_items_acc: history_items_acc,
                            history_total_acc: history_total_acc,
                            history_domain_total: history_domain_total,
                            hist_paging: hist_paging,
                            toast_msg: toast_msg,
                            search_query: search_query,
                            latest_filter: latest_filter,
                        }
                    }
                }

                if let Some(saved) = direct_return() {
                    evidence_navigation::RetainedAttemptDrawer {
                        saved,
                        on_close: move |_| { direct_return.set(None); *EVALUATION_RETURN.write() = None; nav.push(Route::EvaluationsView {}); },
                    }
                } else if let Some(target) = drawer_target.read().clone() {
                    EvalDrawer {
                        target: target,
                        refresh: active_refresh,
                        on_close: move |_| { drawer_target.set(None); *EVALUATION_RETURN.write() = None; },
                        toast_msg: toast_msg,
                        page_history: active_tab() == EvaluationsTab::History,
                        page_search: search_query(),
                        page_status: history_status_filter(),
                        page_flake: history_flake_filter(),
                        page_latest: latest_filter().enabled(),
                        on_open_policy: move |policy_name: String| {
                            navigation_focus.set(Some(NavigationFocus {
                                target: FocusTarget::Policies,
                                commit_sha: None,
                                flake_name: None,
                                status: None,
                                policy_name: Some(policy_name),
                            }));
                            nav.push(Route::PoliciesView {});
                        },
                    }
                }

                // Bulk cancel bar for active queue multi-select (matching JSX BulkBar)
                if active_tab() == EvaluationsTab::ActiveQueue && active_selected_visible_count > 0 {
                    div {
                        class: "ed-bulkbar",
                        span {
                            style: "font-size: 13px; font-weight: 600;",
                            "{active_selected_visible_count} selected"
                        }
                        div { style: "flex: 1;" }
                        button {
                            class: "btn btn-danger xs focus-ring",
                            onclick: move |_| {
                                let ids = active_selected_visible_ids.clone();
                                let mut refresh_sig = active_refresh.clone();
                                let mut selected_sig = active_selected_ids.clone();
                                let mut toast = toast_msg.clone();
                                spawn(async move {
                                    let mut success = 0u32;
                                    let mut failed = Vec::new();
                                    for commit_id in &ids {
                                        match cancel_commit_evaluation(*commit_id).await {
                                            Ok(_) => success += 1,
                                            Err(e) => failed.push((*commit_id, e.to_string())),
                                        }
                                    }
                                    if success > 0 {
                                        toast.set(Some(format!("Cancelled {} evaluation{}", success, if success == 1 { "" } else { "s" })));
                                        selected_sig.set(failed.iter().map(|(id, _)| *id).collect());
                                    } else {
                                        toast.set(Some("Failed to cancel evaluations — see server logs".to_string()));
                                    }
                                    refresh_sig.set(refresh_sig() + 1);
                                });
                            },
                            Icon { name: IconName::X, size: 12 }
                            {
                                let n = active_selected_visible_count;
                                format!(" Cancel {} eval{}", n, if n == 1 { "" } else { "s" })
                            }
                        }
                    }
                }

                // Toast notification (matching JSX ed-toast)
                if let Some(msg) = toast_msg.read().as_ref() {
                    div {
                        class: "ed-toast",
                        span { style: "color: #34d399;", Icon { name: IconName::Check, size: 14 } }
                        span { "{msg}" }
                    }
                }

                div {
                    class: "ed-kbd-hint",
                    span {
                        kbd { "j" }
                        kbd { "k" }
                        " navigate"
                    }
                    span {
                        kbd { "↵" }
                        " open"
                    }
                    if active_tab() == EvaluationsTab::ActiveQueue {
                        span {
                            kbd { "c" }
                            " cancel"
                        }
                    }
                }
        }
    }
}

#[component]
fn EvalActiveQueue(
    evals: Vec<EvalQueueItem>,
    mut refresh: Signal<u64>,
    queue_items: Memo<Vec<EvalQueueItem>>,
    mut drawer_target: Signal<Option<EvalDrawerTarget>>,
    mut focused_index: Signal<Option<usize>>,
    mut active_selected_ids: Signal<std::collections::HashSet<i32>>,
    mut toast_msg: Signal<Option<String>>,
    allow_reorder: bool,
) -> Element {
    // Drag-and-drop reorder state (matching JSX dragId/overIdx)
    let mut drag_id = use_signal(|| None::<i32>);
    let mut over_idx = use_signal(|| None::<usize>);

    if evals.is_empty() {
        return rsx! {
            div { class: "q-empty",
                h3 { "No active evaluations" }
                div { "All flake evaluations are complete." }
            }
        };
    }

    rsx! {
        table {
            class: "sys-table q-queue-table",
            thead {
                tr {
                    th { style: "width: 64px;", "#" }
                    th { "Flake · commit" }
                    th { "Branch" }
                    th { "Status" }
                    th { "Systems" }
                    th { "Policy" }
                    th { "Started" }
                    th { style: "text-align: right;", "Reorder · actions" }
                }
            }
            tbody {
                for (i , ev) in evals.iter().enumerate() {
                    {
                        let ev_clone = ev.clone();
                        let commit_id = ev.commit_id;
                        let status_meta = eval_status_meta(&ev.evaluation_status);
                        let can_cancel = matches!(ev.evaluation_status.as_str(), "pending" | "in_progress");
                        let is_first = i == 0;
                        let is_last = i == evals.len() - 1;
                        let ev_for_row = ev_clone.clone();
                        let is_focused = focused_index() == Some(i);
                        let checked = active_selected_ids.read().contains(&commit_id);
                        let is_dragging = drag_id() == Some(commit_id);
                        let drag_idx = drag_id().and_then(|id| evals.iter().position(|e| e.commit_id == id));
                        let show_drop_before = drag_id().is_some() && over_idx() == Some(i) && drag_idx.map(|d| d > i).unwrap_or(false);
                        let show_drop_after = drag_id().is_some() && over_idx() == Some(i) && drag_idx.map(|d| d < i).unwrap_or(false);

                        let mut row_draw = drawer_target.clone();
                        let row_ev = ev_for_row.clone();
                        let row_onclick = move |_| {
                            row_draw.set(Some(EvalDrawerTarget::Queue(row_ev.clone())));
                        };

                        let row_classes = format!(
                            "selectable q-row{}{}{}{}{}",
                            if is_focused { " selected" } else { "" },
                            if checked { " row-checked" } else { "" },
                            if is_dragging { " q-dragging" } else { "" },
                            if show_drop_before { " q-drop-before" } else { "" },
                            if show_drop_after { " q-drop-after" } else { "" },
                        );

                        rsx! {
                            tr {
                                key: "{commit_id}",
                                id: evaluation_row_id(false, commit_id),
                                tabindex: "0",
                                onfocus: move |_| focused_index.set(Some(i)),
                                class: "{row_classes}",
                                draggable: if allow_reorder { "true" } else { "false" },
                                ondragstart: move |_| {
                                    if allow_reorder { drag_id.set(Some(commit_id)); }
                                },
                                ondragover: move |evt| {
                                    evt.prevent_default();
                                    if allow_reorder && over_idx() != Some(i) { over_idx.set(Some(i)); }
                                },
                                ondrop: move |evt| {
                                    evt.prevent_default();
                                    let src_id = allow_reorder.then(|| drag_id()).flatten();
                                    if let Some(src_id) = src_id {
                                        let target_cid = commit_id;
                                        if src_id == target_cid { return; }
                                        let items = queue_items.clone();
                                        let mut refresh_sig = refresh.clone();
                                        let mut drag_sig = drag_id.clone();
                                        let mut over_sig = over_idx.clone();
                                        let mut toast = toast_msg.clone();
                                        spawn(async move {
                                            let active: Vec<_> = items.read().iter()
                                                .filter(|item| is_active_eval_status(&item.evaluation_status))
                                                .cloned().collect();
                                            let src_pos = active.iter().position(|e| e.commit_id == src_id);
                                            let target_pos = active.iter().position(|e| e.commit_id == target_cid);
                                            if let (Some(sp), Some(tp)) = (src_pos, target_pos) {
                                                let mut reordered = active.clone();
                                                let removed = reordered.remove(sp);
                                                let adjusted_tp = if sp < tp { tp - 1 } else { tp };
                                                reordered.insert(adjusted_tp, removed);
                                                let ordered_ids: Vec<i32> = reordered.iter().map(|e| e.commit_id).collect();
                                                if let Err(e) = reorder_eval_queue(&ordered_ids).await {
                                                    toast.set(Some(format!("Reorder failed: {}", e)));
                                                } else {
                                                    refresh_sig.set(refresh_sig() + 1);
                                                }
                                            }
                                        });
                                        drag_sig.set(None);
                                        over_sig.set(None);
                                    }
                                },
                                onclick: row_onclick,
                                td {
                                    onclick: move |evt| evt.stop_propagation(),
                                    div { style: "display: flex; align-items: center; gap: 6px;",
                                        span {
                                            class: "q-drag-handle",
                                            title: if allow_reorder { "Drag to reorder" } else { "Reordering is unavailable while filtering" },
                                            Icon { name: IconName::Grip, size: 15 }
                                        }
                                        span {
                                            style: "color: var(--cf-text-muted); font-size: 12px; font-variant-numeric: tabular-nums;",
                                            "{ev.queue_position}"
                                        }
                                    }
                                }
                                td {
                                    div { style: "font-weight: 600; font-size: 13px;", "{ev_clone.flake_name}" }
                                    div {
                                        class: if ev_clone.is_latest_per_flake { "mono commit-latest" } else { "mono" },
                                        style: "font-size: 11px; color: var(--cf-text-muted); display: flex; align-items: center; gap: 0;",
                                        if ev_clone.is_latest_per_flake {
                                            span { class: "latest-star", style: "display: inline-flex; align-items: center; margin-right: 3px; flex-shrink: 0;",
                                                Icon { name: IconName::Star, size: 9 }
                                            }
                                        }
                                        "{ev_clone.commit_hash.chars().take(12).collect::<String>()}"
                                    }
                                }
                                td { span { class: "chip chip-unknown", "{ev_clone.branch}" } }
                                td {
                                    span {
                                        class: "chip {status_meta.cls}",
                                        span { class: "chip-dot", style: "background: {status_meta.color};" }
                                        "{status_meta.label}"
                                    }
                                }
                                td { style: "font-size: 12px; color: var(--cf-text-secondary);", "{format_inventory_count(ev_clone.inventory_available, ev_clone.system_count)}" }
                                td {
                                    div { style: "display: flex; gap: 6px;",
                                        PolicyCounts { available: ev_clone.policy_counts_available, passed: ev_clone.passed_count, failed: ev_clone.policy_failed_count }
                                    }
                                }
                                td { style: "font-size: 12px; color: var(--cf-text-muted);", "{format_relative_time(ev_clone.committed_at)}" }
                                td {
                                    onclick: move |evt| evt.stop_propagation(),
                                    div {
                                        class: "row-actions",
                                        style: "opacity: 1; gap: 6px; justify-content: flex-end;",
                                        div { class: "q-move-group",
                                            button {
                                                class: "q-move-btn focus-ring",
                                                title: "Move up",
                                                disabled: is_first || !allow_reorder,
                                                onclick: move |_| {
                                                    if is_first || !allow_reorder { return; }
                                                    let items = queue_items.clone();
                                                    let mut refresh_sig = refresh.clone();
                                                    let mut toast = toast_msg.clone();
                                                    spawn(async move {
                                                        let active: Vec<_> = items.read().iter()
                                                            .filter(|item| is_active_eval_status(&item.evaluation_status))
                                                            .cloned().collect();
                                                        if let Some(idx) = active.iter().position(|e| e.commit_id == commit_id) {
                                                            if idx > 0 {
                                                                let mut reordered = active;
                                                                let removed = reordered.remove(idx);
                                                                reordered.insert(idx - 1, removed);
                                                                let ordered_ids: Vec<i32> = reordered.iter().map(|e| e.commit_id).collect();
                                                                if let Err(e) = reorder_eval_queue(&ordered_ids).await {
                                                                    toast.set(Some(format!("Reorder failed: {}", e)));
                                                                } else {
                                                                    refresh_sig.set(refresh_sig() + 1);
                                                                }
                                                            }
                                                        }
                                                    });
                                                },
                                                Icon { name: IconName::ChevronUp, size: 15 }
                                            }
                                            button {
                                                class: "q-move-btn focus-ring",
                                                title: "Move down",
                                                disabled: is_last || !allow_reorder,
                                                onclick: move |_| {
                                                    if is_last || !allow_reorder { return; }
                                                    let items = queue_items.clone();
                                                    let mut refresh_sig = refresh.clone();
                                                    let mut toast = toast_msg.clone();
                                                    spawn(async move {
                                                        let active: Vec<_> = items.read().iter()
                                                            .filter(|item| is_active_eval_status(&item.evaluation_status))
                                                            .cloned().collect();
                                                        if let Some(idx) = active.iter().position(|e| e.commit_id == commit_id) {
                                                            if idx + 1 < active.len() {
                                                                let mut reordered = active;
                                                                let removed = reordered.remove(idx);
                                                                reordered.insert(idx, removed);
                                                                let ordered_ids: Vec<i32> = reordered.iter().map(|e| e.commit_id).collect();
                                                                if let Err(e) = reorder_eval_queue(&ordered_ids).await {
                                                                    toast.set(Some(format!("Reorder failed: {}", e)));
                                                                } else {
                                                                    refresh_sig.set(refresh_sig() + 1);
                                                                }
                                                            }
                                                        }
                                                    });
                                                },
                                                Icon { name: IconName::ChevronDown, size: 15 }
                                            }
                                        }
                                        if can_cancel {
                                            button {
                                                class: "btn btn-ghost focus-ring",
                                                style: "padding: 3px 8px; font-size: 11px;",
                                                onclick: move |_| {
                                                    let mut refresh_sig = refresh.clone();
                                                    let mut toast = toast_msg.clone();
                                                    spawn(async move {
                                                        if let Err(e) = cancel_commit_evaluation(commit_id).await {
                                                            toast.set(Some(format!("Cancel failed: {}", e)));
                                                        } else {
                                                            refresh_sig.set(refresh_sig() + 1);
                                                        }
                                                    });
                                                },
                                                "Cancel"
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
fn EvalHistory(
    history_resource: Memo<Option<Result<EvalHistoryPage, String>>>,
    visible_rows: Memo<Vec<EvalHistoryItem>>,
    mut history_status_filter: Signal<String>,
    mut history_flake_filter: Signal<String>,
    mut history_fetch_limit: Signal<i64>,
    mut refresh: Signal<u64>,
    mut history_selected_ids: Signal<std::collections::HashSet<i32>>,
    mut drawer_target: Signal<Option<EvalDrawerTarget>>,
    mut focused_index: Signal<Option<usize>>,
    flash_evals: bool,
    history_items_acc: Memo<Vec<EvalHistoryItem>>,
    history_total_acc: Memo<i64>,
    history_domain_total: Memo<i64>,
    hist_paging: InfiniteScroll,
    mut toast_msg: Signal<Option<String>>,
    mut search_query: Signal<String>,
    mut latest_filter: Signal<LatestFilterState>,
) -> Element {
    let history_snapshot = history_resource.read();

    // Compute server-more state locally for the render (sentinel visibility).
    // The server caps list queries at FETCH_LIMIT_MAX, so totals beyond that
    // are unreachable — stop advertising "more" at the cap.
    let hist_server_has_more = {
        let loaded = history_items_acc.read().len();
        let total = history_total_acc();
        (loaded as i64) < total.min(FETCH_LIMIT_MAX)
    };

    rsx! {
        div {
            // Filter bar
            div {
                style: "padding: 12px 16px; border-bottom: 1px solid var(--cf-divider); display: flex; gap: 10px; flex-wrap: wrap; align-items: center;",

                div {
                    class: "seg",
                    for (label, value) in [("all", "all"), ("complete", "complete"), ("failed", "failed"), ("cancelled", "cancelled")] {
                        {
                            let value_str = value.to_string();
                            let is_active = history_status_filter() == value;
                            rsx! {
                                button {
                                    key: "{label}",
                                    class: if is_active { "active" } else { "" },
                                    onclick: move |_| {
                                        history_status_filter.set(value_str.clone());
                                        history_fetch_limit.set(50);
                                        refresh.set(refresh() + 1);
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                }

                select {
                    class: "input filter-select focus-ring",
                    style: "width: auto;",
                    value: "{history_flake_filter()}",
                    onchange: move |evt| {
                        history_flake_filter.set(evt.value().clone());
                        history_fetch_limit.set(50);
                        refresh.set(refresh() + 1);
                    },
                    option { value: "all", selected: history_flake_filter() == "all", "All flakes" }
                        {
                            let selected = history_flake_filter();
                            let mut flakes: Vec<String> = history_snapshot
                                .as_ref()
                                .and_then(|result| result.as_ref().ok())
                                .map(|page| page.items.iter().map(|item| item.flake_name.clone()).collect())
                                .unwrap_or_default();
                            // Keep the selected option mounted while its owned
                            // page reloads; otherwise the browser resets to All.
                            if selected != "all" && !selected.is_empty() {
                                flakes.push(selected.clone());
                            }
                            flakes.sort();
                            flakes.dedup();
                            flakes.into_iter().map(move |flake_name| rsx! {
                                option {
                                    key: "{flake_name}",
                                     value: "{flake_name}",
                                     selected: flake_name == selected,
                                    "{flake_name}"
                                }
                            })
                        }
                }

                if history_total_acc() > 0 {
                    span {
                        class: "filter-count",
                        "{history_total_acc()} of {history_domain_total()} entries"
                    }
                }
            }

            // History table
            match &*history_snapshot {
                Some(Ok(page_data)) => rsx! {
                    {
                        // Use accumulated items (across server pages) for the table.
                        // page_data is still used for the flake filter dropdown above
                        // and for loading/error state detection.
                        let paged_items = visible_rows();
                        let hist_has_more = hist_paging.count() < history_items_acc.read().len() || hist_server_has_more;
                        rsx! {
                    table {
                        class: "sys-table",
                        thead {
                            tr {
                                th { "Flake · commit" }
                                th { "Branch" }
                                th { "Status" }
                                th { "Systems" }
                                th { "Policy" }
                                th { "Duration" }
                                th { "Completed" }
                                th { style: "text-align: right;", " " }
                            }
                        }
                        tbody {
                            for (row_i, ev) in paged_items.iter().enumerate() {
                                {
                                    let ev = ev.clone();
                                    let commit_id = ev.commit_id;
                                    let status_meta = eval_status_meta(&ev.evaluation_status);
                                    let ev_for_row = ev.clone();
                                    let is_focused = focused_index() == Some(row_i);
                                    let is_selected = history_selected_ids.read().contains(&commit_id);

                                    let is_failed = ev.evaluation_status == "failed";
                                    // Resolve the same way dismiss_attention_item
                                    // resolves its local key: prefer the
                                    // canonical server occurrence key (keyed by
                                    // commit_id + evaluation_completed_at
                                    // microseconds server-side), falling back
                                    // to the commit id. This must stay in sync
                                    // with the identity dismiss_attention_item
                                    // stores, or a re-evaluation that fails
                                    // again would either fail to clear on
                                    // click (mismatched key) or stay
                                    // permanently hidden (stale local entry
                                    // from a resolved prior occurrence).
                                    let commit_id_str = commit_id.to_string();
                                    let eval_key = occurrence_id_for_subject("evals", &commit_id_str)
                                        .unwrap_or(commit_id_str);
                                    let row_class = attention_row_class(
                                        match (is_focused, is_selected) {
                                            (true, true) => "kbd-focused row-checked",
                                            (true, false) => "kbd-focused",
                                            (false, true) => "row-checked",
                                            (false, false) => "",
                                        },
                                        "evals",
                                        &eval_key,
                                        is_failed,
                                        is_failed && flash_evals,
                                    );

                                    rsx! {
                                        tr {
                                            key: "{commit_id}",
                                            id: evaluation_row_id(true, commit_id),
                                            tabindex: "0",
                                            onfocus: move |_| focused_index.set(Some(row_i)),
                                            class: "{row_class}",
                                            aria_selected: is_selected,
                                            style: "cursor: pointer;",
                                            onclick: move |evt| {
                                                // Shift-click toggles selection (design: no checkboxes,
                                                // selection via row click).
                                                if evt.modifiers().shift() {
                                                    let mut next = history_selected_ids.read().clone();
                                                    if next.contains(&commit_id) {
                                                        next.remove(&commit_id);
                                                    } else {
                                                        next.insert(commit_id);
                                                    }
                                                    history_selected_ids.set(next);
                                                } else {
                                                    if is_failed {
                                                        dismiss_attention_item(
                                                            "evals",
                                                            &commit_id.to_string(),
                                                            occurrence_id_for_subject("evals", &commit_id.to_string()).as_deref(),
                                                        );
                                                    }
                                                    drawer_target.set(Some(EvalDrawerTarget::History(ev_for_row.clone())));
                                                }
                                            },
                                            td {
                                                div {
                                                    style: "font-weight: 600; font-size: 13px; display: flex; align-items: center; gap: 6px;",
                                                    span { style: "color: var(--cf-text-muted);", Icon { name: IconName::Git, size: 12 } }
                                                    "{ev.flake_name}"
                                                }
                                                div {
                                                    class: if ev.is_latest_per_flake { "mono commit-latest" } else { "mono" },
                                                    style: "font-size: 11px; color: var(--cf-text-muted); display: flex; align-items: center; gap: 0;",
                                                    if ev.is_latest_per_flake {
                                                        span { class: "latest-star", style: "display: inline-flex; align-items: center; margin-right: 3px; flex-shrink: 0;",
                                                            Icon { name: IconName::Star, size: 9 }
                                                        }
                                                    }
                                                    "{ev.commit_hash.chars().take(12).collect::<String>()}"
                                                }
                                            }
                                            td {
                                                span { class: "chip chip-unknown", "{ev.branch}" }
                                            }
                                            td {
                                                span {
                                                    class: "chip {status_meta.cls}",
                                                    span {
                                                        class: "chip-dot",
                                                        style: "background: {status_meta.color};"
                                                    }
                                                    "{status_meta.label}"
                                                }
                                            }
                                            td {
                                                style: "font-size: 12px;",
                                                "{format_inventory_count(ev.inventory_available, ev.system_count)}"
                                            }
                                            td {
                                                div {
                                                    style: "display: flex; gap: 6px;",
                                                    PolicyCounts { available: ev.policy_counts_available, passed: ev.passed_count, failed: ev.policy_failed_count }
                                                }
                                            }
                                            td {
                                                class: "mono",
                                                style: "font-size: 12px; color: var(--cf-text-secondary);",
                                                "{format_eval_duration(&ev)}"
                                            }
                                            td {
                                                style: "font-size: 12px; color: var(--cf-text-muted);",
                                                "{format_eval_completed_at(&ev)}"
                                            }
                                            td {
                                                onclick: move |evt| evt.stop_propagation(),
                                                div {
                                                    class: "row-actions",
                                                    // View logs
                                                    button {
                                                        class: "btn-icon focus-ring",
                                                        title: "View logs",
                                                        onclick: {
                                                            let ev_log = ev.clone();
                                                            move |_| {
                                                                drawer_target.set(Some(EvalDrawerTarget::History(ev_log.clone())));
                                                            }
                                                        },
                                                        Icon { name: IconName::Terminal, size: 14 }
                                                    }
                                                    if is_failed {
                                                        button {
                                                            class: "btn-icon focus-ring",
                                                            title: "Retry evaluation",
                                                            onclick: move |_| {
                                                                let mut refresh_sig = refresh.clone();
                                                                let mut toast = toast_msg.clone();
                                                                spawn(async move {
                                                                    match re_evaluate_commit(commit_id).await {
                                                                        Ok(_) => {
                                                                            toast.set(Some("Retrying evaluation".to_string()));
                                                                            refresh_sig.set(refresh_sig() + 1);
                                                                        }
                                                                        Err(_) => {
                                                                            toast.set(Some("Retry failed — see server logs".to_string()));
                                                                        }
                                                                    }
                                                                });
                                                            },
                                                            Icon { name: IconName::Rollback, size: 14 }
                                                        }
                                                    } else {
                                                        button {
                                                            class: "btn-icon focus-ring",
                                                            title: "Re-evaluate",
                                                            onclick: move |_| {
                                                                let mut refresh_sig = refresh.clone();
                                                                let mut toast = toast_msg.clone();
                                                                spawn(async move {
                                                                    match re_evaluate_commit(commit_id).await {
                                                                        Ok(_) => {
                                                                            toast.set(Some("Re-queued evaluation".to_string()));
                                                                            refresh_sig.set(refresh_sig() + 1);
                                                                        }
                                                                        Err(_) => {
                                                                            toast.set(Some("Re-evaluate failed — see server logs".to_string()));
                                                                        }
                                                                    }
                                                                });
                                                            },
                                                            Icon { name: IconName::Sync, size: 14 }
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
                    if hist_has_more {
                        div {
                            class: "infinite-sentinel",
                            "data-sentinel": hist_paging.sentinel_id(),
                            onmounted: move |_| hist_paging.check_and_register(),
                            "Loading more…"
                        }
                    }
                    if page_data.domain_total == 0 {
                        div { class: "q-empty",
                            h3 { "No completed evaluations" }
                            div { "Completed evaluations will appear here." }
                        }
                    } else if page_data.total_count == 0 {
                        div { class: "q-empty",
                            Icon { name: IconName::Search, size: 20 }
                            h3 { "No matching evaluations" }
                            div { "Try adjusting your search or filters." }
                            button {
                                class: "btn btn-ghost xs focus-ring",
                                onclick: move |_| {
                                    history_status_filter.set("all".to_string());
                                    history_flake_filter.set("all".to_string());
                                    search_query.set(String::new());
                                    latest_filter.with_mut(LatestFilterState::clear);
                                },
                                "Clear active filters"
                            }
                        }
                    }
                        }
                    }
                },
                Some(Err(e)) => rsx! {
                    div {
                        style: "padding: 24px; color: var(--cf-red);",
                        "Failed to load eval history: {e}"
                    }
                },
                None => rsx! {
                    div {
                        style: "padding: 24px; display: flex; align-items: center; gap: 12px; color: var(--cf-text-muted);",
                        "Loading history…"
                    }
                }
            }
        }
    }
}

#[component]
fn EvalDrawer(
    target: EvalDrawerTarget,
    mut refresh: Signal<u64>,
    on_close: EventHandler<()>,
    mut toast_msg: Signal<Option<String>>,
    on_open_policy: EventHandler<String>,
    page_history: bool,
    page_search: String,
    page_status: String,
    page_flake: String,
    page_latest: bool,
) -> Element {
    let saved = use_hook(|| {
        EVALUATION_RETURN
            .peek()
            .clone()
            .filter(|saved| match &target {
                EvalDrawerTarget::Queue(item) => {
                    saved.matches_commit(item.commit_id)
                        && item.attempt_number == saved.identity.attempt_number
                }
                EvalDrawerTarget::History(item) => {
                    saved.matches_commit(item.commit_id)
                        && item.attempt_number == saved.identity.attempt_number
                }
            })
    });
    let original_tab = saved
        .as_ref()
        .map(|s| s.tab.clone())
        .unwrap_or_else(|| "log".into());
    let pinned_view = saved.is_some();
    let saved_tab = handoff::retained_tab(&original_tab, pinned_view);
    let nav = navigator();
    let mut drawer_tab = use_signal(|| saved_tab);
    let maximized = use_signal(|| saved.as_ref().is_some_and(|s| s.maximized));
    let target_for_context = target.clone();
    // All matrix actions share this owner snapshot. The exact attempt is added
    // only after the matrix response has validated its server context.
    let return_seed = EvaluationReturn {
        identity: EvaluationIdentity {
            commit_id: match &target {
                EvalDrawerTarget::Queue(i) => i.commit_id,
                EvalDrawerTarget::History(i) => i.commit_id,
            },
            commit_sha: match &target {
                EvalDrawerTarget::Queue(i) => i.commit_hash.clone(),
                EvalDrawerTarget::History(i) => i.commit_hash.clone(),
            },
            attempt_id: saved
                .as_ref()
                .map(|s| s.identity.attempt_id)
                .unwrap_or_else(uuid::Uuid::nil),
            attempt_number: match &target {
                EvalDrawerTarget::Queue(i) => i.attempt_number,
                EvalDrawerTarget::History(i) => i.attempt_number,
            },
        },
        target: Some(target_for_context),
        history: page_history,
        search: page_search,
        status: page_status,
        flake: page_flake,
        latest: page_latest,
        tab: if pinned_view {
            original_tab
        } else {
            drawer_tab()
        },
        maximized: maximized(),
        system: None,
        dom: None,
    };
    let mut return_context = use_context_provider(|| Signal::new(return_seed.clone()));
    use_effect(use_reactive(&return_seed, move |seed| {
        if *return_context.peek() != seed {
            return_context.set(seed);
        }
    }));
    // Bind confirmation to the selected commit, so polling or navigation cannot
    // transfer confirmation to a different evaluation.
    let mut force_cancel_target = use_signal(|| None::<i32>);
    let mut force_cancel_busy = use_signal(|| false);

    match target {
        EvalDrawerTarget::Queue(ev) => {
            let status_meta = eval_status_meta(&ev.evaluation_status);
            let can_cancel =
                !pinned_view && matches!(ev.evaluation_status.as_str(), "pending" | "in_progress");
            let can_force_cancel = !pinned_view && ev.evaluation_status == "cancelling";
            let is_live = matches!(
                ev.evaluation_status.as_str(),
                "pending" | "in_progress" | "cancelling"
            );
            let close_click = move |_: MouseEvent| on_close.call(());

            rsx! {
                div {
                    class: "fl-tray-backdrop",
                    onclick: close_click,
                }
                aside {
                    id: "queue-evaluation-dialog",
                    aria_modal: "true",
                    tabindex: "-1",
                    onkeydown: move |event| if event.key() == Key::Escape { event.stop_propagation(); on_close.call(()); },
                    class: if maximized() { "fl-tray ed-drawer fl-tray-max" } else { "fl-tray ed-drawer" },
                    role: "dialog",
                    "aria-label": "Evaluation detail",
                    "data-commit-sha": "{ev.commit_hash}",
                    "data-attempt-number": "{ev.attempt_number}",
                    DialogFocusRestore {}
                    DialogInitialFocus { dialog_id: "queue-evaluation-dialog" }
                    DialogFocusSentinel { dialog_id: "queue-evaluation-dialog", boundary: DialogFocusBoundary::Last }
                    header {
                        class: "fl-tray-head",
                        div {
                            style: "display: flex; align-items: center; gap: 12px; min-width: 0; flex: 1;",
                            span { class: "ed-identity-icon", Icon { name: IconName::Evaluation, size: 18 } }
                            div { style: "min-width: 0;",
                            h2 { class: "ed-identity-title",
                                span { class: "ed-flake-name", "{ev.flake_name}" }
                                span { class: "chip chip-unknown", style: "font-size: 10px;", "{ev.branch}" }
                                span {
                                    class: "chip {status_meta.cls}",
                                    span { class: "chip-dot", style: "background: {status_meta.color};" }
                                    "{status_meta.label}"
                                    if is_live { span { class: "ed-pulse" } }
                                }
                            }
                            div {
                                class: "mono ed-identity-meta",
                                "{ev.commit_hash} · {ev.commit_id}"
                                if ev.attempt_number > 1 {
                                    span {
                                        class: "chip chip-unknown",
                                        style: "font-size: 10px; margin-left: 6px;",
                                        title: "Automatic/manual retry attempt {ev.attempt_number}",
                                        "attempt {ev.attempt_number}"
                                    }
                                }
                            }
                            }
                        }
                        div {
                            style: "display: flex; gap: 6px; align-items: center;",
                            if can_cancel {
                                button {
                                    class: "btn btn-ghost focus-ring xs",
                                    onclick: move |_| {
                                        let mut refresh_sig = refresh.clone();
                                        let mut toast = toast_msg.clone();
                                        let commit_id = ev.commit_id;
                                        spawn(async move {
                                            if let Err(e) = cancel_commit_evaluation(commit_id).await {
                                                toast.set(Some(format!("Cancel failed: {}", e)));
                                            } else {
                                                refresh_sig.set(refresh_sig() + 1);
                                            }
                                        });
                                    },
                                    "Cancel"
                                }
                            }
                            if can_force_cancel {
                                button {
                                    class: "btn btn-ghost focus-ring xs",
                                    style: "color: #f87171;",
                                    disabled: force_cancel_busy(),
                                    onclick: move |_| force_cancel_target.set(Some(ev.commit_id)),
                                    "Force-cancel"
                                }
                            }
                            if pinned_view { button { class: "btn btn-ghost xs focus-ring", title: "This drawer is pinned to a retained attempt. Open the latest list for evaluation actions.", onclick: move |_| { *EVALUATION_RETURN.write() = None; nav.push(Route::EvaluationsView {}); }, "Open latest evaluations" } }
                            drawer_header::ExpandButton { maximized }
                            button {
                                class: "btn-icon focus-ring",
                                onclick: close_click,
                                 title: "Close",
                                 aria_label: "Close",
                                Icon { name: IconName::X, size: 16 }
                            }
                        }
                    }
                    div {
                        class: "ed-stats",
                        // The DTO has no actual start time. Queue time must not
                        // be presented as evaluation start or commit time.
                        div { class: "ed-stat", div { class: "ed-stat-label", "Queued" } div { class: "ed-stat-val ed-stat-time", drawer_header::Timestamp { timestamp: ev.enqueued_at } } }
                        div { class: "ed-stat", div { class: "ed-stat-label", "Duration" } div { class: "ed-stat-val mono", "—" } }
                        div { class: "ed-stat", title: "Visible planned and discovered configurations for this exact attempt.", div { class: "ed-stat-label", "Systems" } InventoryCount { available: ev.inventory_available, count: ev.system_count, live: is_live } }
                        div {
                            class: "ed-stat",
                            div { class: "ed-stat-label", "Policy" }
                            DrawerPolicyCounts { available: ev.inventory_available, passed: ev.passed_count, failed: ev.policy_failed_count, pending: ev.policy_pending_count, unavailable: ev.policy_unavailable_count, evaluation_errors: ev.eval_failed_count, excluded: ev.unselected_count, live: is_live }
                        }
                        div { class: "ed-stat", title: "Persisted derivation records for this attempt, not closure packages or outputs. — means unavailable.", div { class: "ed-stat-label", "Derivations" } div { class: "ed-stat-val", "{format_derivation_count(ev.derivation_count)}" } }
                    }
                    div {
                        class: "sd-tabs",
                        style: "padding: 0 16px; border-bottom: 1px solid var(--cf-card-border); flex-shrink: 0;",
                        button {
                            class: if drawer_tab() == "log" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            disabled: pinned_view,
                            aria_disabled: pinned_view,
                            aria_describedby: if pinned_view { "retained-log-unavailable" } else { "" },
                            title: if pinned_view { "Log unavailable for retained attempt: no exact-attempt log API is available." } else { "Evaluation log" },
                            onclick: move |_| drawer_tab.set("log".to_string()),
                            Icon { name: IconName::EvaluationLog, size: 12 }
                            if pinned_view { " Log unavailable for retained attempt" } else { " Log" }
                            if is_live { span { class: "ed-pulse" } }
                        }
                        button {
                            class: if drawer_tab() == "policy" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            onclick: move |_| drawer_tab.set("policy".to_string()),
                            Icon { name: IconName::Shield, size: 12 }
                            " Policy matrix"
                        }
                        button {
                            class: if drawer_tab() == "graph" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            disabled: pinned_view,
                            aria_disabled: pinned_view,
                            aria_describedby: if pinned_view { "retained-graph-unavailable" } else { "" },
                            title: if pinned_view { "Dependency graph unavailable for retained attempt: no exact-attempt graph API is available." } else { "Dependency graph" },
                            onclick: move |_| drawer_tab.set("graph".to_string()),
                            Icon { name: IconName::EvaluationGraph, size: 12 }
                            " Dependency graph"
                        }
                    }
                    div {
                        class: "ed-body",
                        if pinned_view { evidence_navigation::RetainedTabReasons {} }
                        if pinned_view && drawer_tab() != "policy" {
                            p { role: "note", "Pinned attempt log and dependency evidence are unavailable from the current endpoints." }
                        } else if drawer_tab() == "log" {
                            EvalDrawerLogTabQueue { ev: ev.clone(), live: is_live }
                        } else if drawer_tab() == "policy" {
                            EvalDrawerPolicyTab {
                                key: "{ev.commit_id}",
                                commit_id: ev.commit_id,
                                live: is_live,
                                on_open_policy: on_open_policy,
                            }
                        } else {
                            EvalDrawerGraphTab {
                                commit_id: ev.commit_id,
                            }
                        }
                    }
                    DialogFocusSentinel { dialog_id: "queue-evaluation-dialog", boundary: DialogFocusBoundary::First }
                }
                if can_force_cancel && force_cancel_target() == Some(ev.commit_id) {
                    drawer_header::ConfirmForceCancel {
                        commit_id: ev.commit_id,
                        on_cancel: move |_| force_cancel_target.set(None),
                        on_confirm: move |_| {
                            if *force_cancel_busy.peek() || *force_cancel_target.peek() != Some(ev.commit_id) {
                                return;
                            }
                            force_cancel_target.set(None);
                            force_cancel_busy.set(true);
                            let mut refresh_sig = refresh.clone();
                            let mut toast = toast_msg.clone();
                            let commit_id = ev.commit_id;
                            spawn(async move {
                                if let Err(e) = force_cancel_commit_evaluation(commit_id).await {
                                    toast.set(Some(format!("Force cancel failed: {}", e)));
                                } else {
                                    refresh_sig.set(refresh_sig() + 1);
                                }
                                force_cancel_busy.set(false);
                            });
                        },
                    }
                }
            }
        }
        EvalDrawerTarget::History(ev) => {
            let status_meta = eval_status_meta(&ev.evaluation_status);
            let (time_label, timestamp) =
                drawer_header::history_timestamp(ev.evaluation_completed_at, ev.enqueued_at);
            let is_live = matches!(
                ev.evaluation_status.as_str(),
                "pending" | "in_progress" | "cancelling"
            );
            let close_click = move |_: MouseEvent| on_close.call(());

            rsx! {
                div {
                    class: "fl-tray-backdrop",
                    onclick: close_click,
                }
                aside {
                    id: "history-evaluation-dialog",
                    aria_modal: "true",
                    tabindex: "-1",
                    onkeydown: move |event| if event.key() == Key::Escape { event.stop_propagation(); on_close.call(()); },
                    class: if maximized() { "fl-tray ed-drawer fl-tray-max" } else { "fl-tray ed-drawer" },
                    role: "dialog",
                    "aria-label": "Evaluation detail",
                    "data-commit-sha": "{ev.commit_hash}",
                    "data-attempt-number": "{ev.attempt_number}",
                    DialogFocusRestore {}
                    DialogInitialFocus { dialog_id: "history-evaluation-dialog" }
                    DialogFocusSentinel { dialog_id: "history-evaluation-dialog", boundary: DialogFocusBoundary::Last }
                    header {
                        class: "fl-tray-head",
                        div {
                            style: "display: flex; align-items: center; gap: 12px; min-width: 0; flex: 1;",
                            span { class: "ed-identity-icon", Icon { name: IconName::Evaluation, size: 18 } }
                            div { style: "min-width: 0;",
                            h2 { class: "ed-identity-title",
                                span { class: "ed-flake-name", "{ev.flake_name}" }
                                span { class: "chip chip-unknown", style: "font-size: 10px;", "{ev.branch}" }
                                span {
                                    class: "chip {status_meta.cls}",
                                    span { class: "chip-dot", style: "background: {status_meta.color};" }
                                    "{status_meta.label}"
                                    if is_live { span { class: "ed-pulse" } }
                                }
                            }
                            div {
                                class: "mono ed-identity-meta",
                                "{ev.commit_hash} · {ev.commit_id}"
                                if ev.attempt_number > 1 {
                                    span {
                                        class: "chip chip-unknown",
                                        style: "font-size: 10px; margin-left: 6px;",
                                        title: "Automatic/manual retry attempt {ev.attempt_number}",
                                        "attempt {ev.attempt_number}"
                                    }
                                }
                            }
                            }
                        }
                        div { style: "display: flex; gap: 6px; align-items: center;",
                        if pinned_view { button { class: "btn btn-ghost xs focus-ring", title: "This drawer is pinned to a retained attempt. Open the latest list for evaluation actions.", onclick: move |_| { *EVALUATION_RETURN.write() = None; nav.push(Route::EvaluationsView {}); }, "Open latest evaluations" } }
                        drawer_header::ExpandButton { maximized }
                        button {
                            class: "btn-icon focus-ring",
                            onclick: close_click,
                             title: "Close",
                             aria_label: "Close",
                            Icon { name: IconName::X, size: 16 }
                        }
                        }
                    }
                    div {
                        class: "ed-stats",
                        // Completion is authoritative when present. The DTO has
                        // no start time; older rows fall back to queued time.
                        div { class: "ed-stat",
                            div { class: "ed-stat-label", "{time_label}" }
                            div { class: "ed-stat-val ed-stat-time",
                                drawer_header::Timestamp { timestamp }
                            }
                        }
                        div { class: "ed-stat", div { class: "ed-stat-label", "Duration" } div { class: "ed-stat-val mono", "{format_eval_duration(&ev)}" } }
                        div { class: "ed-stat", title: "Visible planned and discovered configurations for this exact attempt.", div { class: "ed-stat-label", "Systems" } InventoryCount { available: ev.inventory_available, count: ev.system_count, live: is_live } }
                        div {
                            class: "ed-stat",
                            div { class: "ed-stat-label", "Policy" }
                            DrawerPolicyCounts { available: ev.inventory_available, passed: ev.passed_count, failed: ev.policy_failed_count, pending: ev.policy_pending_count, unavailable: ev.policy_unavailable_count, evaluation_errors: ev.eval_failed_count, excluded: ev.unselected_count, live: is_live }
                        }
                        div { class: "ed-stat", title: "Persisted derivation records for this attempt, not closure packages or outputs. — means unavailable.", div { class: "ed-stat-label", "Derivations" } div { class: "ed-stat-val", "{format_derivation_count(ev.derivation_count)}" } }
                    }
                    div {
                        class: "sd-tabs",
                        style: "padding: 0 16px; border-bottom: 1px solid var(--cf-card-border); flex-shrink: 0;",
                        button {
                            class: if drawer_tab() == "log" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            disabled: pinned_view,
                            aria_disabled: pinned_view,
                            aria_describedby: if pinned_view { "retained-log-unavailable" } else { "" },
                            title: if pinned_view { "Log unavailable for retained attempt: no exact-attempt log API is available." } else { "Evaluation log" },
                            onclick: move |_| drawer_tab.set("log".to_string()),
                            Icon { name: IconName::EvaluationLog, size: 12 }
                            if pinned_view { " Log unavailable for retained attempt" } else { " Log" }
                            if is_live { span { class: "ed-pulse" } }
                        }
                        button {
                            class: if drawer_tab() == "policy" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            onclick: move |_| drawer_tab.set("policy".to_string()),
                            Icon { name: IconName::Shield, size: 12 }
                            " Policy matrix"
                        }
                        button {
                            class: if drawer_tab() == "graph" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            disabled: pinned_view,
                            aria_disabled: pinned_view,
                            aria_describedby: if pinned_view { "retained-graph-unavailable" } else { "" },
                            title: if pinned_view { "Dependency graph unavailable for retained attempt: no exact-attempt graph API is available." } else { "Dependency graph" },
                            onclick: move |_| drawer_tab.set("graph".to_string()),
                            Icon { name: IconName::EvaluationGraph, size: 12 }
                            " Dependency graph"
                        }
                    }
                    div {
                        class: "ed-body",
                        if pinned_view { evidence_navigation::RetainedTabReasons {} }
                        if pinned_view && drawer_tab() != "policy" {
                            p { role: "note", "Pinned attempt log and dependency evidence are unavailable from the current endpoints." }
                        } else if drawer_tab() == "log" {
                            EvalDrawerLogTabHistory { ev: ev.clone(), live: is_live }
                        } else if drawer_tab() == "policy" {
                            EvalDrawerPolicyTab {
                                key: "{ev.commit_id}",
                                commit_id: ev.commit_id,
                                live: is_live,
                                on_open_policy: on_open_policy,
                            }
                        } else {
                            EvalDrawerGraphTab {
                                commit_id: ev.commit_id,
                            }
                        }
                    }
                    div {
                        class: "panel-actions",
                        if !pinned_view && ev.evaluation_status != "complete" {
                            button {
                                class: "btn btn-ghost focus-ring",
                                onclick: move |_| {
                                    let mut refresh_sig = refresh.clone();
                                    let mut toast = toast_msg.clone();
                                    let commit_id = ev.commit_id;
                                    spawn(async move {
                                        if let Err(e) = re_evaluate_commit(commit_id).await {
                                            toast.set(Some(format!("Re-evaluate failed: {}", e)));
                                        } else {
                                            refresh_sig.set(refresh_sig() + 1);
                                        }
                                    });
                                },
                                Icon { name: IconName::Sync, size: 14 }
                                " Re-evaluate"
                            }
                        }
                    }
                    DialogFocusSentinel { dialog_id: "history-evaluation-dialog", boundary: DialogFocusBoundary::First }
                }
            }
        }
    }
}

#[component]
fn EvalDrawerLogTabQueue(ev: EvalQueueItem, live: bool) -> Element {
    let mut autoscroll = use_signal(|| true);
    let commit_id = ev.commit_id;
    let mut poll_tick = use_signal(|| 0_u64);

    // When live, poll every 2 s to pick up new log lines
    {
        use_future(move || async move {
            #[cfg(target_arch = "wasm32")]
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                    if live {
                        poll_tick.set(poll_tick() + 1);
                    } else {
                        break;
                    }
                }
            }
        });
    }

    // Re-fetch whenever poll_tick changes (driven by the loop above when live)
    let logs_resource = use_resource(move || async move {
        let _ = poll_tick();
        crate::api::client::fetch_eval_logs(commit_id).await
    });

    let logs_snapshot = logs_resource.read();
    let lines: Vec<String> = match &*logs_snapshot {
        Some(Ok(entries)) => entries
            .iter()
            .map(|e| {
                let level = e
                    .level
                    .as_ref()
                    .map(|s| s.to_uppercase())
                    .unwrap_or_else(|| "INFO".to_string());
                format!(
                    "{} [{}] {}",
                    e.timestamp.format("%Y-%m-%d %H:%M:%S%.3fZ"),
                    level,
                    e.message
                )
            })
            .collect(),
        Some(Err(_)) | None => Vec::new(),
    };
    let is_loading = logs_snapshot.is_none();

    rsx! {
        div { style: "display: flex; flex-direction: column; flex: 1; min-height: 0;",
            div {
                style: "padding: 8px 16px; border-bottom: 1px solid var(--cf-divider); display: flex; gap: 10px; align-items: center; flex-shrink: 0;",
                span { style: "font-size: 11px; color: var(--cf-text-muted);", "{lines.len()} lines" }
                div { style: "flex: 1;" }
                label {
                    style: "display: flex; gap: 6px; align-items: center; font-size: 11px;",
                    input {
                        r#type: "checkbox",
                        class: "ed-checkbox",
                        checked: autoscroll(),
                        oninput: move |_| autoscroll.set(!autoscroll()),
                    }
                    "Auto-scroll"
                }
                button {
                    class: "btn-icon focus-ring",
                    title: "Download",
                    Icon { name: IconName::Download, size: 13 }
                }
            }
            pre { class: "fl-diff",
                if is_loading {
                    div { style: "color: var(--cf-text-muted); padding: 12px;", "Loading logs..." }
                } else if lines.is_empty() {
                    div { style: "color: var(--cf-text-muted); padding: 12px;",
                        if live { "Waiting for log output..." } else { "No logs available" }
                    }
                } else {
                    for (idx, line) in lines.iter().enumerate() {
                        {
                            let color = log_line_color(line);
                            let line_num = format!("{:>3}", idx + 1);
                            rsx! {
                                div { style: "color: {color};",
                                    span { style: "color: var(--cf-text-muted); user-select: none; display: inline-block; width: 36px;", "{line_num}" }
                                    " {line}"
                                }
                            }
                        }
                    }
                }
                if live {
                    {
                        let next_line_num = format!("{:>3}", lines.len() + 1);
                        rsx! {
                            div { style: "color: #60a5fa;",
                                span { style: "color: var(--cf-text-muted); display: inline-block; width: 36px;", "{next_line_num}" }
                                " "
                                span { class: "ed-pulse", style: "margin-left: 0;" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EvalDrawerLogTabHistory(ev: EvalHistoryItem, live: bool) -> Element {
    let mut autoscroll = use_signal(|| true);
    let commit_id = ev.commit_id;
    let mut poll_tick = use_signal(|| 0_u64);

    // When live (cancelling state), poll every 2 s
    {
        use_future(move || async move {
            #[cfg(target_arch = "wasm32")]
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                    if live {
                        poll_tick.set(poll_tick() + 1);
                    } else {
                        break;
                    }
                }
            }
        });
    }

    // Re-fetch whenever poll_tick changes
    let logs_resource = use_resource(move || async move {
        let _ = poll_tick();
        crate::api::client::fetch_eval_logs(commit_id).await
    });

    let logs_snapshot = logs_resource.read();
    let lines: Vec<String> = match &*logs_snapshot {
        Some(Ok(entries)) => entries
            .iter()
            .map(|e| {
                let level = e
                    .level
                    .as_ref()
                    .map(|s| s.to_uppercase())
                    .unwrap_or_else(|| "INFO".to_string());
                format!(
                    "{} [{}] {}",
                    e.timestamp.format("%Y-%m-%d %H:%M:%S%.3fZ"),
                    level,
                    e.message
                )
            })
            .collect(),
        Some(Err(_)) | None => Vec::new(),
    };
    let is_loading = logs_snapshot.is_none();

    rsx! {
        div { style: "display: flex; flex-direction: column; flex: 1; min-height: 0;",
            div {
                style: "padding: 8px 16px; border-bottom: 1px solid var(--cf-divider); display: flex; gap: 10px; align-items: center; flex-shrink: 0;",
                span { style: "font-size: 11px; color: var(--cf-text-muted);", "{lines.len()} lines" }
                div { style: "flex: 1;" }
                label {
                    style: "display: flex; gap: 6px; align-items: center; font-size: 11px;",
                    input {
                        r#type: "checkbox",
                        class: "ed-checkbox",
                        checked: autoscroll(),
                        oninput: move |_| autoscroll.set(!autoscroll()),
                    }
                    "Auto-scroll"
                }
                button {
                    class: "btn-icon focus-ring",
                    title: "Download",
                    Icon { name: IconName::Download, size: 13 }
                }
            }
            pre { class: "fl-diff",
                if is_loading {
                    div { style: "color: var(--cf-text-muted); padding: 12px;", "Loading logs..." }
                } else if lines.is_empty() {
                    div { style: "color: var(--cf-text-muted); padding: 12px;",
                        if let Some(error) = ev.evaluation_error_message.as_deref() {
                            "Evaluation failed before log streaming began: {error}"
                        } else if live {
                            "Waiting for log output..."
                        } else {
                            "No logs available"
                        }
                    }
                } else {
                    for (idx, line) in lines.iter().enumerate() {
                        {
                            let color = log_line_color(line);
                            let line_num = format!("{:>3}", idx + 1);
                            rsx! {
                                div { style: "color: {color};",
                                    span { style: "color: var(--cf-text-muted); user-select: none; display: inline-block; width: 36px;", "{line_num}" }
                                    " {line}"
                                }
                            }
                        }
                    }
                }
                if live {
                    {
                        let next_line_num = format!("{:>3}", lines.len() + 1);
                        rsx! {
                            div { style: "color: #60a5fa;",
                                span { style: "color: var(--cf-text-muted); display: inline-block; width: 36px;", "{next_line_num}" }
                                " "
                                span { class: "ed-pulse", style: "margin-left: 0;" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Canonical category for a raw policy-matrix cell status string returned by
/// the backend (`pass`, `fail`, `warn`, `not_assigned`, `infrastructure_error`,
/// `nix_eval_failure`, `legacy_unknown`). Every raw status must map to
/// exactly one category so the UI can never silently treat an unrecognized
/// or infrastructure-level status as "clean".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PolicyCellCategory {
    Pass,
    Fail,
    Warn,
    /// Policy was not assigned to this configuration. Excluded from the
    /// health denominator; never shown as a failure.
    NotAssigned,
    /// Historical row predating the persisted policy-results model
    /// (migration 0185). Requires re-evaluation; excluded from the health
    /// denominator; never counted as passing.
    LegacyUnknown,
}

/// Map a raw backend status string to its display category.
///
/// `fail`, `infrastructure_error`, and `nix_eval_failure` all block
/// deployment and are grouped as `Fail` for counting/coloring purposes (the
/// raw string is still shown in the tooltip/detail text). Any unrecognized
/// status also falls back to `Fail` — fail-closed, so a new backend status
/// added without a matching UI case is never silently counted as passing.
fn policy_cell_category(raw: &str) -> PolicyCellCategory {
    match raw {
        "pass" => PolicyCellCategory::Pass,
        "warn" => PolicyCellCategory::Warn,
        "not_assigned" => PolicyCellCategory::NotAssigned,
        "legacy_unknown" => PolicyCellCategory::LegacyUnknown,
        "fail" | "infrastructure_error" | "nix_eval_failure" => PolicyCellCategory::Fail,
        _ => PolicyCellCategory::Fail,
    }
}

/// True when this cell represents an actually-evaluated policy outcome
/// (pass/fail/warn) and should count toward a system's health ratio.
/// `not_assigned` and `legacy_unknown` carry no signal and must not affect
/// the apparent pass ratio.
fn policy_cell_counts_toward_health(raw: &str) -> bool {
    !matches!(
        policy_cell_category(raw),
        PolicyCellCategory::NotAssigned | PolicyCellCategory::LegacyUnknown
    )
}

fn policy_cell_glyph(raw: &str) -> &'static str {
    match policy_cell_category(raw) {
        PolicyCellCategory::Pass => "✓",
        PolicyCellCategory::Fail => "✗",
        PolicyCellCategory::Warn => "!",
        PolicyCellCategory::NotAssigned => "–",
        PolicyCellCategory::LegacyUnknown => "?",
    }
}

/// CSS modifier suffix (without the `pm-` prefix) for a policy-matrix cell,
/// driven by category rather than the raw backend status string. This is
/// what makes `infrastructure_error`, `nix_eval_failure`, `not_assigned`,
/// and `legacy_unknown` get real styling instead of silently rendering
/// unstyled (and therefore looking indistinguishable from "clean").
fn policy_cell_class_suffix(raw: &str) -> &'static str {
    match policy_cell_category(raw) {
        PolicyCellCategory::Pass => "pass",
        PolicyCellCategory::Fail => "fail",
        PolicyCellCategory::Warn => "warn",
        PolicyCellCategory::NotAssigned => "not-assigned",
        PolicyCellCategory::LegacyUnknown => "legacy-unknown",
    }
}

/// Renders the selected commit's matrix and refreshes live partial outcomes.
///
/// The keyed tab owns the request and browser timer. Reactive commit/live changes
/// cancel the previous resource future. Setting live to false triggers one final
/// fetch; unmount drops request futures and timers. Live failures remain visible
/// and retry after five seconds. Historical failures expose an explicit retry.
/// Pinned reads retain their UUID across refreshes and stop at terminal status.
#[component]
fn EvalDrawerPolicyTab(
    commit_id: i32,
    on_open_policy: EventHandler<String>,
    #[props(default)] live: bool,
) -> Element {
    // Keep the displayed response outside Resource: restarting a resource can
    // clear its value and unmount CapturedMatrix, losing presentation choices.
    // Publish only completed requests, including errors; never mask an explicit
    // failure with cached successful data. Key the cache as well as the tab so
    // even a reused component cannot display a previous commit during a fetch.
    let mut displayed = use_signal(|| None);
    let mut request_generation = use_signal(|| 0u64);
    let pinned = use_hook(move || {
        EVALUATION_RETURN
            .peek()
            .as_ref()
            .filter(|r| r.identity.commit_id == commit_id)
            .map(|r| r.identity.clone())
    });
    let pinned_request = pinned.clone();
    let _policy_resource = use_resource(use_reactive(
        &(commit_id, live, request_generation()),
        move |(commit_id, live, _generation)| {
            let pinned = pinned_request.clone();
            async move {
                loop {
                    let result = if let Some(identity) = &pinned {
                        crate::api::client::fetch_eval_attempt_policy_matrix(
                            commit_id,
                            identity.attempt_id,
                        )
                        .await
                    } else {
                        fetch_eval_policy_matrix(commit_id).await
                    };
                    let pinned_terminal = pinned.is_some()
                        && result.as_ref().is_ok_and(|data| {
                            data.matrix
                                .as_ref()
                                .and_then(|snapshot| snapshot.attempt_status.as_deref())
                                .is_some_and(|status| {
                                    matches!(
                                        status,
                                        "complete"
                                            | "completed"
                                            | "succeeded"
                                            | "failed"
                                            | "cancelled"
                                    )
                                })
                        });
                    displayed.set(Some((commit_id, result)));
                    if !live || pinned_terminal {
                        break;
                    }
                    // Browser-only polling avoids native timers in WASM. A delay
                    // after each response prevents overlapping requests and bounds
                    // retries even when the endpoint fails immediately.
                    #[cfg(target_arch = "wasm32")]
                    TimeoutFuture::new(5_000).await;
                    #[cfg(not(target_arch = "wasm32"))]
                    break;
                }
            }
        },
    ));
    let displayed = displayed.read();
    let policy_snapshot = displayed
        .as_ref()
        .filter(|(id, _)| *id == commit_id)
        .map(|(_, response)| response);

    // Presentation belongs to the request owner, not its success branch. An
    // explicit failure unmounts evidence but retains these same-attempt choices.
    let mut matrix_state = use_signal(|| policy_matrix::ColumnState::new(0));
    let mut matrix_expanded = use_signal(|| None::<String>);
    let mut matrix_issue = use_signal(|| None::<String>);
    let mut matrix_finding = use_signal(|| None::<String>);
    let mut matrix_page = use_signal(|| 0usize);
    let mut matrix_key = use_signal(|| None::<(i32, uuid::Uuid)>);
    let mut matrix_initialized = use_signal(|| false);
    let captured_snapshot = policy_snapshot
        .and_then(|r| r.as_ref().ok())
        .filter(|data| data.commit_id == commit_id)
        .and_then(|r| r.matrix.clone());
    let snapshot_for_effect = captured_snapshot.clone();
    use_effect(use_reactive(&snapshot_for_effect, move |snapshot| {
        let Some(snapshot) = snapshot else {
            return;
        };
        let Some(id) = snapshot.evaluation_attempt_id else {
            return;
        };
        let key = (commit_id, id);
        // Only capture changes drive initialization. Subscribing to an unchanged
        // provisional false flag would repeatedly publish sparse defaults and
        // prevent the same attempt's first usable capture from being processed.
        if *matrix_key.peek() != Some(key) {
            let mut saved = handoff::MATRIX_PRESENTATIONS
                .peek()
                .get(&key)
                .cloned()
                .unwrap_or_else(|| handoff::MatrixPresentation::new(0));
            saved.initialize(&snapshot);
            matrix_initialized.set(saved.initialized);
            matrix_state.set(saved.columns);
            matrix_expanded.set(saved.expanded);
            matrix_issue.set(saved.issue);
            matrix_finding.set(saved.finding);
            matrix_page.set(saved.page);
            matrix_key.set(Some(key));
        } else if !*matrix_initialized.peek() {
            let mut defaults = handoff::MatrixPresentation::new(0);
            defaults.initialize(&snapshot);
            if defaults.initialized {
                matrix_state.set(defaults.columns);
                matrix_initialized.set(true);
            }
        }
    }));
    use_effect(move || {
        let Some(key) = matrix_key() else {
            return;
        };
        let state = matrix_state();
        handoff::MATRIX_PRESENTATIONS.write().insert(
            key,
            handoff::MatrixPresentation {
                initialized: matrix_initialized(),
                columns: state,
                expanded: matrix_expanded(),
                issue: matrix_issue(),
                finding: matrix_finding(),
                page: matrix_page(),
            },
        );
    });
    let valid_return = pinned.as_ref().is_none_or(|identity| {
        policy_snapshot.is_some_and(|response| {
            response
                .as_ref()
                .is_ok_and(|data| data.commit_id == identity.commit_id)
        }) && captured_snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.evaluation_attempt_id == Some(identity.attempt_id)
                && snapshot.attempt_number == identity.attempt_number
                && snapshot.commit_sha.as_ref() == Some(&identity.commit_sha)
        })
    });
    let mut dom_restored = use_signal(|| false);
    // Validation is a derived value, not a signal read. Track it explicitly so
    // the initially pending pinned response cannot leave this effect dormant.
    use_effect(use_reactive(&valid_return, move |valid_return| {
        if valid_return && matrix_initialized() && matrix_key().is_some() && !dom_restored() {
            if let Some(dom) = EVALUATION_RETURN
                .peek()
                .as_ref()
                .and_then(|s| s.dom.clone())
            {
                dom_restored.set(true);
                spawn(handoff::restore_dom(dom));
            }
        }
    }));

    // Interactive state: filter, sort, expanded row, policy filter (matches JSX)
    let mut filter_state = use_signal(|| "all".to_string());
    let mut sort_state = use_signal(|| "health".to_string());
    let mut expanded = use_signal(|| None::<String>);
    let mut open_cause = use_signal(|| None::<String>);
    let mut policy_filter = use_signal(|| None::<String>);

    rsx! {
        div {
            style: "flex: 1; overflow: hidden; display: flex; flex-direction: column;",
            if policy_snapshot.is_some_and(|response| response.as_ref().is_ok_and(|data| data.matrix.as_ref().is_some_and(|matrix| matrix.mock_evidence))) {
                div { class: "pm-provenance-note", role: "note",
                    "Mock evaluation — simulated outcomes, not compliance evidence."
                }
            }
            match policy_snapshot {
                None => rsx! {
                    div { style: "color: var(--cf-text-muted); font-size: 12px; padding: 14px;", "Loading policy matrix..." }
                },
                Some(Err(_)) => rsx! {
                    div { role: "alert", style: "color: #f87171; font-size: 12px; padding: 14px;", "Failed to load policy matrix — evidence unavailable." }
                    button { class: "btn btn-ghost xs focus-ring", onclick: move |_| request_generation += 1, "Retry matrix request" }
                },
                Some(Ok(data)) if data.commit_id != commit_id || !valid_return => rsx! { div { role: "alert", "Exact evaluation attempt unavailable or context mismatch. Saved presentation was not applied to another attempt." } },
                Some(Ok(data)) if data.matrix.is_some() => rsx! {
                    if let Some(snapshot) = &data.matrix {
                        // A new captured attempt intentionally resets local
                        // choices; successful refreshes of that attempt do not.
                        policy_captured::CapturedMatrix { key: "{snapshot.evaluation_attempt_id:?}:{snapshot.attempt_number}", snapshot: snapshot.clone(), on_open_policy, state: matrix_state, expanded: matrix_expanded, issue: matrix_issue, finding: matrix_finding, page: matrix_page }
                    }
                },
                Some(Ok(data)) => {
                    let policies = &data.policies;
                    let base_rows = &data.systems;

                    // Annotate rows with counts (matching JSX annotated).
                    // fail/warn/pass are derived from the cell's canonical
                    // category, not a literal string match, so
                    // infrastructure_error/nix_eval_failure count as fail and
                    // not_assigned/legacy_unknown count toward neither. Health
                    // is measured against `evaluated` (policies that actually
                    // produced a pass/fail/warn signal for this system), not
                    // the raw policy count, so unassigned/legacy-unknown
                    // columns cannot deflate a system's apparent pass ratio.
                    struct AnnotatedRow {
                        system_name: String,
                        results: Vec<String>,
                        details: Vec<Option<String>>,
                        fail: usize,
                        warn: usize,
                        pass: usize,
                        evaluated: usize,
                        legacy_unknown: usize,
                    }

                    impl AnnotatedRow {
                        // Missing legacy coverage and empty applicability are
                        // never sufficient evidence for a clean system.
                        fn is_clean(&self) -> bool {
                            policy_matrix::legacy_row_is_clean(
                                self.pass, self.fail, self.warn, self.legacy_unknown,
                            )
                        }
                    }

                    let annotated: Vec<AnnotatedRow> = base_rows.iter().map(|r| {
                        let fail = r.results.iter().filter(|x| policy_cell_category(x) == PolicyCellCategory::Fail).count();
                        let warn = r.results.iter().filter(|x| policy_cell_category(x) == PolicyCellCategory::Warn).count();
                        let pass = r.results.iter().filter(|x| policy_cell_category(x) == PolicyCellCategory::Pass).count();
                        let evaluated = r.results.iter().filter(|x| policy_cell_counts_toward_health(x)).count();
                        let legacy_unknown = r.results.iter().filter(|x| policy_cell_category(x) == PolicyCellCategory::LegacyUnknown).count();
                        AnnotatedRow {
                            system_name: r.system_name.clone(),
                            results: r.results.clone(),
                            details: r.details.clone(),
                            fail,
                            warn,
                            pass,
                            evaluated,
                            legacy_unknown,
                        }
                    }).collect();

                    // Apply filter (matching JSX)
                    let filtered = {
                        let f = filter_state.read().clone();
                        let pf = policy_filter.read().clone();
                        let mut result: Vec<&AnnotatedRow> = annotated.iter().collect();
                        match f.as_str() {
                            "fail" => result.retain(|r| r.fail > 0),
                            "warn" => result.retain(|r| r.warn > 0 && r.fail == 0),
                            // "Clean" means every evaluated policy passed AND
                            // there are no legacy_unknown cells requiring
                            // re-evaluation — a system with unknown history
                            // must never be reported as clean.
                            "clean" => result.retain(|r| r.is_clean()),
                            _ => {}
                        }
                        if let Some(ref policy_name) = pf {
                            if let Some(idx) = policies.iter().position(|p| p == policy_name) {
                                result.retain(|r| r.results.get(idx).map_or(false, |res| {
                                    !matches!(
                                        policy_cell_category(res),
                                        PolicyCellCategory::Pass | PolicyCellCategory::NotAssigned
                                    )
                                }));
                            }
                        }
                        // Sort
                        let sort = sort_state.read().clone();
                        if sort == "health" {
                            result.sort_by(|a, b| (b.fail * 10 + b.warn).cmp(&(a.fail * 10 + a.warn)));
                        } else {
                            result.sort_by(|a, b| a.system_name.cmp(&b.system_name));
                        }
                        result
                    };

                    // Per-policy summary (matching JSX policyStats). `total`
                    // is the number of systems for which this policy was
                    // actually evaluated (excludes not_assigned/legacy_unknown)
                    // so the fail/warn/pass percentage bar reflects real signal.
                    struct PolicyStat {
                        name: String,
                        fail: usize,
                        warn: usize,
                        pass: usize,
                        total: usize,
                    }
                    let policy_stats: Vec<PolicyStat> = policies.iter().enumerate().map(|(i, name)| {
                        let fail = annotated.iter().filter(|r| r.results.get(i).map_or(false, |x| policy_cell_category(x) == PolicyCellCategory::Fail)).count();
                        let warn = annotated.iter().filter(|r| r.results.get(i).map_or(false, |x| policy_cell_category(x) == PolicyCellCategory::Warn)).count();
                        let pass = annotated.iter().filter(|r| r.results.get(i).map_or(false, |x| policy_cell_category(x) == PolicyCellCategory::Pass)).count();
                        let evaluated = annotated.iter().filter(|r| r.results.get(i).map_or(false, |x| policy_cell_counts_toward_health(x))).count();
                        PolicyStat { name: name.clone(), fail, warn, pass, total: evaluated }
                    }).collect();

                    // Top issues — top 3 most-failed policies (matching JSX)
                    let top_issues: Vec<&PolicyStat> = policy_stats.iter()
                        .filter(|s| s.fail > 0)
                        .collect::<Vec<_>>();

                    let top_issues_sorted = {
                        let mut v = top_issues.clone();
                        v.sort_by(|a, b| b.fail.cmp(&a.fail));
                        v.into_iter().take(3).collect::<Vec<_>>()
                    };

                    // Counts for filter badges
                    let count_fail = annotated.iter().filter(|r| r.fail > 0).count();
                    let count_warn = annotated.iter().filter(|r| r.fail == 0 && r.warn > 0).count();
                    let count_clean = annotated.iter().filter(|r| r.is_clean()).count();

                    let cell_glyph = policy_cell_glyph;

                    rsx! {
                        div { class: "pm-provenance-note", role: "note",
                            "Historical bundle provenance unavailable. These are flat compatibility results; bundle memberships and complete historical applicability cannot be reconstructed from check names."
                        }
                        // Top issues callout (matching JSX)
                        if !top_issues_sorted.is_empty() {
                            div {
                                class: "pm-issues",
                                div { class: "pm-issues-label", "Top issues" }
                                {top_issues_sorted.iter().map(|iss| {
                                    let is_active = policy_filter.read().as_ref().map(|f| f == &iss.name).unwrap_or(false);
                                    let name = iss.name.clone();
                                    let mut pf = policy_filter.clone();
                                    let click_iss = move |_| {
                                        if pf.read().as_ref().map(|f| f == &name).unwrap_or(false) {
                                            pf.set(None);
                                        } else {
                                            pf.set(Some(name.clone()));
                                        }
                                    };
                                    let counts = format!("{}/{} fail", iss.fail, iss.total);
                                    rsx! {
                                        button {
                                            key: "{iss.name}",
                                            class: if is_active { "pm-issue-chip active" } else { "pm-issue-chip" },
                                            onclick: click_iss,
                                            "{iss.name} ({counts})"
                                        }
                                    }
                                })}
                                {if policy_filter.read().is_some() {
                                    Some(rsx! {
                                        button {
                                            class: "btn-icon focus-ring",
                                            style: "margin-left: auto;",
                                            title: "Clear policy filter",
                                            onclick: move |_| policy_filter.set(None),
                                            Icon { name: IconName::X, size: 12 }
                                        }
                                    })
                                } else {
                                    None
                                }}
                            }
                        }

                        // Controls: filter seg + sort seg (matching JSX)
                        div {
                            class: "pm-controls",
                            div {
                                class: "seg",
                                {
                                    let f_all = move |_| { filter_state.set("all".to_string()); };
                                    let f_fail = move |_| { filter_state.set("fail".to_string()); };
                                    let f_warn = move |_| { filter_state.set("warn".to_string()); };
                                    let f_clean = move |_| { filter_state.set("clean".to_string()); };
                                    let f_cur = filter_state.read().clone();
                                    rsx! {
                                        button { class: if f_cur == "all" { "active" } else { "" }, onclick: f_all, "All ", span { class: "pm-count", "{annotated.len()}" } }
                                        button { class: if f_cur == "fail" { "active" } else { "" }, onclick: f_fail, "Failing ", span { class: "pm-count pm-count-fail", "{count_fail}" } }
                                        button { class: if f_cur == "warn" { "active" } else { "" }, onclick: f_warn, "Warning ", span { class: "pm-count pm-count-warn", "{count_warn}" } }
                                        button { class: if f_cur == "clean" { "active" } else { "" }, onclick: f_clean, "Clean ", span { class: "pm-count pm-count-pass", "{count_clean}" } }
                                    }
                                }
                            }
                            div { style: "flex: 1;" }
                            span { style: "font-size: 11px; color: var(--cf-text-muted);", "Sort" }
                            {
                                let s_health = move |_| { sort_state.set("health".to_string()); };
                                let s_name = move |_| { sort_state.set("name".to_string()); };
                                let s_cur = sort_state.read().clone();
                                rsx! {
                                    div { class: "seg",
                                        button { class: if s_cur == "health" { "active" } else { "" }, onclick: s_health, "Worst first" }
                                        button { class: if s_cur == "name" { "active" } else { "" }, onclick: s_name, "Name" }
                                    }
                                }
                            }
                        }

                        // Scrollable matrix table (matching JSX pm-scroll > pm-table)
                        div {
                            class: "pm-scroll",
                            table { class: "pm-table",
                                thead {
                                    tr {
                                        th { class: "pm-th-host", "System" }
                                        th { class: "pm-th-health", "Health" }
                                        for (p_idx, policy) in policies.iter().enumerate() {
                                            {
                                                let ps = &policy_stats[p_idx];
                                                let is_filtered = policy_filter.read().as_ref() == Some(policy);
                                                let click_header = {
                                                    let mut pf = policy_filter.clone();
                                                    let name = policy.clone();
                                                    move |_| {
                                                        if pf.read().as_ref() == Some(&name) {
                                                            pf.set(None);
                                                        } else {
                                                            pf.set(Some(name.clone()));
                                                        }
                                                    }
                                                };
                                                let fail_pct = if ps.total > 0 { (ps.fail as f64 / ps.total as f64) * 100.0 } else { 0.0 };
                                                let warn_pct = if ps.total > 0 { (ps.warn as f64 / ps.total as f64) * 100.0 } else { 0.0 };
                                                let pass_pct = if ps.total > 0 { (ps.pass as f64 / ps.total as f64) * 100.0 } else { 0.0 };
                                                rsx! {
                                                    th {
                                                        key: "{policy}",
                                                        class: if is_filtered { "pm-th-policy filtered" } else { "pm-th-policy" },
                                                        title: "{policy} — {ps.fail} fail / {ps.warn} warn / {ps.pass} pass",
                                                        onclick: click_header,
                                                        div { class: "pm-th-policy-inner",
                                                            span { class: "pm-th-policy-label", "{policy}" }
                                                        }
                                                        div { class: "pm-th-policy-bar",
                                                            if ps.fail > 0 { div { style: "width: {fail_pct}%; background: #f87171;" } }
                                                            if ps.warn > 0 { div { style: "width: {warn_pct}%; background: #f59e0b;" } }
                                                            if ps.pass > 0 { div { style: "width: {pass_pct}%; background: #34d399;" } }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                tbody {
                                    for row in filtered.iter() {
                                        {
                                            let is_exp = expanded.read().as_ref() == Some(&row.system_name);
                                            let health_color = if row.fail > 0 { "#f87171" } else if row.warn > 0 { "#f59e0b" } else { "#34d399" };
                                            let host = row.system_name.clone();
                                            let click_row = {
                                                let mut exp = expanded.clone();
                                                let h = host.clone();
                                                move |_| {
                                                    if exp.read().as_ref() == Some(&h) {
                                                        exp.set(None);
                                                    } else {
                                                        exp.set(Some(h.clone()));
                                                    }
                                                }
                                            };
                                            let row_class = format!("pm-row{}", if is_exp { " expanded" } else { "" });
                                            rsx! {
                                                tr {
                                                    key: "{host}",
                                                    class: "{row_class}",
                                                    onclick: click_row,
                                                    style: "cursor: pointer;",
                                                    td { class: "pm-td-host",
                                                        div { class: "pm-host-cell",
                                                            span { style: "color: var(--cf-text-muted); flex-shrink: 0; display: flex;",
                                                                Icon {
                                                                    name: if is_exp { IconName::ChevronDown } else { IconName::ChevronRight },
                                                                    size: 11,
                                                                }
                                                            }
                                                            span { class: "mono pm-host-name", "{row.system_name}" }
                                                        }
                                                    }
                                                    td { class: "pm-td-health",
                                                        div { class: "pm-health",
                                                            div { class: "pm-health-bar",
                                                                if row.fail > 0 {
                                                                    div { style: "width: {(row.fail as f64 / row.evaluated.max(1) as f64) * 100.0}%; background: #f87171;" }
                                                                }
                                                                if row.warn > 0 {
                                                                    div { style: "width: {(row.warn as f64 / row.evaluated.max(1) as f64) * 100.0}%; background: #f59e0b;" }
                                                                }
                                                                if row.pass > 0 {
                                                                    div { style: "width: {(row.pass as f64 / row.evaluated.max(1) as f64) * 100.0}%; background: #34d399;" }
                                                                }
                                                            }
                                                            span { class: "mono pm-health-num", style: "color: {health_color};",
                                                                if row.evaluated == 0 {
                                                                    if row.legacy_unknown > 0 { "Unknown" } else { "N/A" }
                                                                } else {
                                                                    "{row.pass}/{row.evaluated}"
                                                                }
                                                            }
                                                        }
                                                    }
                                                    {row.results.iter().enumerate().map(|(res_idx, result)| {
                                                        let policy_name = &policies[res_idx];
                                                        let col_filtered = policy_filter.read().as_ref() == Some(policy_name);
                                                        let cls = format!("pm-td-cell pm-{}{}", policy_cell_class_suffix(result), if col_filtered { " col-filtered" } else { "" });
                                                        let mut pf = policy_filter.clone();
                                                        let name = policy_name.clone();
                                                        let cell_click = move |e: MouseEvent| {
                                                            e.stop_propagation();
                                                            if pf.read().as_ref() == Some(&name) {
                                                                pf.set(None);
                                                            } else {
                                                                pf.set(Some(name.clone()));
                                                            }
                                                        };
                                                        rsx! {
                                                            td {
                                                                key: "{res_idx}",
                                                                class: "{cls}",
                                                                title: "{policy_name}: {result}",
                                                                onclick: cell_click,
                                                                span { class: "pm-glyph", "{cell_glyph(result)}" }
                                                            }
                                                        }
                                                    })}
                                                }
                                                if is_exp {
                                                    tr { class: "pm-expand-row",
                                                        td {
                                                            colspan: policies.len() + 2,
                                                            div { class: "pm-expand",
                                                                 div { style: "display: flex; flex-direction: column; gap: 14px;",
                                                                      {row.results.iter().enumerate()
                                                                          // not_assigned carries no signal and has nothing to
                                                                          // show; every other non-pass category (fail, warn,
                                                                          // infrastructure_error, nix_eval_failure,
                                                                          // legacy_unknown) needs a card so it can never be
                                                                          // silently treated as clean.
                                                                          .filter(|(_, result)| !matches!(
                                                                              policy_cell_category(result),
                                                                              PolicyCellCategory::Pass | PolicyCellCategory::NotAssigned
                                                                          ))
                                                                          .map(|(res_idx, result)| {
                                                                              let policy_name = &policies[res_idx];
                                                                              let class_suffix = policy_cell_class_suffix(result);
                                                                              let failcard_class = format!("pm-failcard pm-failcard-{}", class_suffix);
                                                                              let glyph = cell_glyph(result);
                                                                              let card_key = format!("{}::{}", row.system_name, res_idx);
                                                                              let is_open = open_cause.read().as_ref() == Some(&card_key);
                                                                               let fallback_desc = match policy_cell_category(result) {
                                                                                   PolicyCellCategory::Fail => "Blocks deployment until resolved",
                                                                                   PolicyCellCategory::Warn => "Soft warning — deploy will proceed",
                                                                                   PolicyCellCategory::LegacyUnknown => "Historical result predates policy tracking — re-evaluate to get current status",
                                                                                   PolicyCellCategory::NotAssigned | PolicyCellCategory::Pass => "",
                                                                               };
                                                                               let evidence_text = row
                                                                                   .details
                                                                                   .get(res_idx)
                                                                                   .and_then(|d| d.as_deref());
                                                                               rsx! {
                                                                                   div {
                                                                                       key: "{res_idx}",
                                                                                       style: "border: 1px solid var(--cf-divider); border-radius: 8px; overflow: hidden;",
                                                                                       div {
                                                                                           class: "{failcard_class} focus-ring",
                                                                                           style: "cursor: pointer; border: none; border-radius: 0;",
                                                                                           onclick: {
                                                                                               let mut open_cause = open_cause.clone();
                                                                                               let key = card_key.clone();
                                                                                               move |e: MouseEvent| {
                                                                                                   e.stop_propagation();
                                                                                                   if open_cause.read().as_ref() == Some(&key) {
                                                                                                       open_cause.set(None);
                                                                                                   } else {
                                                                                                       open_cause.set(Some(key.clone()));
                                                                                                   }
                                                                                               }
                                                                                           },
                                                                                           span { class: "pm-failcard-glyph pm-{class_suffix}", "{glyph}" }
                                                                                           div { style: "min-width: 0; text-align: left;",
                                                                                               div { class: "mono", style: "font-weight: 600; font-size: 12px;", "{policy_name}" }
                                                                                               div {
                                                                                                   style: "font-size: 11px; color: var(--cf-text-muted); margin-top: 2px;",
                                                                                                   "{fallback_desc}"
                                                                                               }
                                                                                           }
                                                                                           Icon {
                                                                                               name: if is_open { IconName::ChevronDown } else { IconName::ChevronRight },
                                                                                               size: 12,
                                                                                           }
                                                                                       }
                                                                                       if is_open {
                                                                                           div {
                                                                                               style: "padding: 10px 12px; background: var(--cf-canvas); border-top: 1px solid var(--cf-divider);",
                                                                                               if let Some(evidence) = evidence_text {
                                                                                                   div {
                                                                                                       style: "font-size: 12px; color: var(--cf-text-secondary); line-height: 1.5;",
                                                                                                       "{evidence}"
                                                                                                   }
                                                                                               }
                                                                                               button {
                                                                                                   class: "btn btn-ghost focus-ring xs",
                                                                                                   style: "margin-top: 8px;",
                                                                                                   onclick: {
                                                                                                       let policy_name = policy_name.to_string();
                                                                                                       move |e: MouseEvent| {
                                                                                                           e.stop_propagation();
                                                                                                           on_open_policy.call(policy_name.clone());
                                                                                                       }
                                                                                                   },
                                                                                                   Icon { name: IconName::File, size: 11 }
                                                                                                   " View policy definition"
                                                                                               }
                                                                                           }
                                                                                       }
                                                                                   }
                                                                               }
                                                                           })}

                                                                         if row.is_clean() {
                                                                            div { style: "font-size: 12px; color: #34d399; display: flex; align-items: center; gap: 8px;",
                                                                                Icon { name: IconName::Check, size: 14 }
                                                                                 " All policies pass for this system."
                                                                             }
                                                                         } else if row.evaluated == 0 && row.legacy_unknown == 0 {
                                                                             div { style: "font-size: 12px; color: var(--cf-text-muted);",
                                                                                 "No applicable controls for this system."
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
                                    if filtered.is_empty() {
                                        tr {
                                            td {
                                                colspan: policies.len() + 2,
                                                style: "padding: 24px; text-align: center; color: var(--cf-text-muted); font-size: 13px;",
                                                "No systems match this filter."
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Legend (matching JSX)
                        div {
                            class: "pm-legend",
                            span { span { class: "pm-legend-sw pm-pass", "✓" } " Pass" }
                            span { span { class: "pm-legend-sw pm-warn", "!" } " Warning" }
                            span { span { class: "pm-legend-sw pm-fail", "✗" } " Fail — blocks deploy" }
                            span { span { class: "pm-legend-sw pm-not-assigned", "–" } " Not assigned" }
                            span { span { class: "pm-legend-sw pm-legacy-unknown", "?" } " Unknown — re-evaluate" }
                            span { style: "margin-left: auto; font-size: 11px; color: var(--cf-text-muted);", "Click any policy header to filter · Click a row to expand" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EvalDrawerGraphTab(commit_id: i32) -> Element {
    const GRAPH_PENDING_POLL_MAX: u64 = 60;

    let mut graph_refresh = use_signal(|| 0_u64);
    let mut has_pending_counts = use_signal(|| true);
    let mut graph_pending_polls = use_signal(|| 0_u64);
    let graph_resource = use_resource(move || async move {
        let _ = graph_refresh();
        fetch_eval_dependency_graph(commit_id).await
    });

    use_effect(move || {
        if let Some(Ok(data)) = &*graph_resource.read() {
            let pending = data.packages.iter().any(|p| !p.closure_counted);
            has_pending_counts.set(pending);
            if !pending {
                graph_pending_polls.set(GRAPH_PENDING_POLL_MAX);
            }
        }
    });

    {
        let mut graph_refresh = graph_refresh.clone();
        use_future(move || async move {
            #[cfg(target_arch = "wasm32")]
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                    if has_pending_counts() && graph_pending_polls() < GRAPH_PENDING_POLL_MAX {
                        graph_pending_polls.set(graph_pending_polls() + 1);
                        graph_refresh.set(graph_refresh() + 1);
                    }
                }
            }
        });
    }

    let graph_snapshot = graph_resource.read();

    rsx! {
        div { style: "flex: 1; overflow: auto; padding: 18px;",
            match &*graph_snapshot {
                None => rsx! {
                    div { style: "color: var(--cf-text-muted); font-size: 12px;", "Loading dependency graph..." }
                },
                Some(Err(_)) => rsx! {
                    div { style: "color: #f87171; font-size: 12px;", "Failed to load dependency graph" }
                },
                Some(Ok(data)) => {
                    let systems_total = data.packages.len() as i64;
                    let total_packages: i64 = data.packages.iter().filter(|p| p.closure_counted).map(|p| p.ready_count + p.pending_count + p.failed_count).sum();
                    let packages_cached: i64 = data.packages.iter().filter(|p| p.closure_counted).map(|p| p.ready_count).sum();
                    let packages_to_build: i64 = data.packages.iter().filter(|p| p.closure_counted).map(|p| p.pending_count).sum();
                    let packages_failed: i64 = data.packages.iter().filter(|p| p.closure_counted).map(|p| p.failed_count).sum();
                    let commit_short: String = format!("commit #{}", commit_id);
                    rsx! {
                        // Summary flow: source → eval → systems → package closure counts
                        div { class: "ed-graph-summary",
                            div { class: "ed-graph-node ed-graph-source",
                                Icon { name: IconName::Git, size: 12 }
                                span { class: "mono", "{commit_short}" }
                            }
                            span { style: "color: var(--cf-text-muted);", "→" }
                            div { class: "ed-graph-node ed-graph-eval",
                                Icon { name: IconName::Terminal, size: 12 }
                                "eval"
                            }
                            span { style: "color: var(--cf-text-muted);", "→" }
                            div { class: "ed-graph-node ed-graph-fan",
                                span { style: "font-weight: 700;", "{systems_total}" }
                                span { style: "font-size: 10px; color: var(--cf-text-muted);", "systems" }
                            }
                            if total_packages > 0 {
                                span { style: "color: var(--cf-text-muted);", "→" }
                                div { class: "ed-graph-node ed-graph-fan",
                                    span { style: "font-weight: 700; color: #34d399;", "{packages_cached}" }
                                    span { style: "font-size: 10px; color: var(--cf-text-muted);", "cached/local" }
                                }
                                div { class: "ed-graph-node ed-graph-fan",
                                    span { style: "font-weight: 700; color: #60a5fa;", "{packages_to_build}" }
                                    span { style: "font-size: 10px; color: var(--cf-text-muted);", "to build" }
                                }
                                if packages_failed > 0 {
                                    div { class: "ed-graph-node ed-graph-fan",
                                        span { style: "font-weight: 700; color: #f87171;", "{packages_failed}" }
                                        span { style: "font-size: 10px; color: var(--cf-text-muted);", "failed" }
                                    }
                                }
                            }
                        }

                        // Per-system breakdown list
                        div { style: "display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 8px;",
                            h3 { style: "font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--cf-text-muted); font-weight: 700; margin: 0;",
                                "Systems evaluated"
                            }
                            span { style: "font-size: 11px; color: var(--cf-text-muted);",
                                "{data.packages.len()} systems"
                            }
                        }

                        if data.packages.is_empty() {
                            div { style: "color: var(--cf-text-muted); font-size: 12px;", "Dependency inventory is unavailable for this evaluation." }
                        } else {
                            div { class: "ed-graph-list",
                                for pkg in data.packages.iter() {
                                    {
                                        let row_total = pkg.ready_count + pkg.pending_count + pkg.failed_count;
                                        let has_counts = pkg.closure_counted && row_total > 0;
                                        let cached_pct = if has_counts { pkg.ready_count * 100 / row_total } else { 0 };
                                        let build_pct = if has_counts { pkg.pending_count * 100 / row_total } else { 0 };
                                        let failed_pct = if has_counts { pkg.failed_count * 100 / row_total } else { 0 };
                                        rsx! {
                                            div { key: "{pkg.package_name}", class: "ed-graph-row",
                                                div { class: "ed-graph-pkg",
                                                    span { class: "mono truncate", style: "font-size: 12px; font-weight: 600;", "{pkg.package_name}" }
                                                    if has_counts {
                                                        span { style: "font-size: 10px; color: var(--cf-text-muted);",
                                                            "{pkg.ready_count}/{row_total} cached/local · {pkg.pending_count} to build"
                                                            if pkg.failed_count > 0 { " · {pkg.failed_count} failed" }
                                                        }
                                                    } else if pkg.failed_count > 0 {
                                                        span { style: "font-size: 10px; color: #f87171;", "failed before closure count" }
                                                    } else {
                                                        span { style: "font-size: 10px; color: #9ca3af;", "pending closure count" }
                                                    }
                                                }
                                                div { class: "ed-graph-bar",
                                                    div { class: "ed-graph-bar-cached", style: "width: {cached_pct}%;" }
                                                    div { class: "ed-graph-bar-build", style: "width: {build_pct}%;" }
                                                    if pkg.failed_count > 0 {
                                                        div { style: "width: {failed_pct}%; background: #f87171;" }
                                                    }
                                                }
                                                if has_counts {
                                                    div { style: "display: flex; gap: 6px; justify-content: flex-end; font-size: 11px;",
                                                        span { style: "color: #34d399; font-weight: 600;", "{pkg.ready_count}" }
                                                        span { style: "color: var(--cf-text-muted);", "·" }
                                                        span { style: "color: #60a5fa; font-weight: 600;", "{pkg.pending_count}" }
                                                        if pkg.failed_count > 0 {
                                                            span { style: "color: var(--cf-text-muted);", "·" }
                                                            span { style: "color: #f87171; font-weight: 600;", "{pkg.failed_count}" }
                                                        }
                                                    }
                                                } else if pkg.failed_count > 0 {
                                                    div { style: "display: flex; justify-content: flex-end; font-size: 11px; color: #f87171; font-weight: 600;", "failed" }
                                                } else {
                                                    div { style: "display: flex; justify-content: flex-end; font-size: 11px; color: #9ca3af;", "—" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "ed-graph-legend",
                                span { span { class: "ed-graph-sw", style: "background: #34d399;" } "Cached/local" }
                                span { span { class: "ed-graph-sw", style: "background: #60a5fa;" } "To build" }
                                if packages_failed > 0 {
                                    span { span { class: "ed-graph-sw", style: "background: #f87171;" } "Failed" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// LiveIndicator — pulsing dot + "updated Ns ago" (matching BuildsView.jsx)
// ============================================================================

#[component]
fn LiveIndicator() -> Element {
    let mut secs = use_signal(|| 0_u64);

    {
        use_future(move || async move {
            #[cfg(target_arch = "wasm32")]
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    gloo_timers::future::TimeoutFuture::new(1000).await;
                    secs.set((secs() + 1) % 6);
                }
            }
        });
    }

    let label = if secs() == 0 {
        "just now".to_string()
    } else {
        format!("{}s ago", secs())
    };

    rsx! {
        div {
            style: "display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--cf-text-muted);",
            span {
                style: "display: inline-flex; align-items: center; gap: 6px;",
                span { class: "ed-pulse", style: "position: static; margin: 0;" }
                span { style: "color: #34d399; font-weight: 600;", "Live" }
            }
            span { "· updated {label}" }
        }
    }
}

// ============================================================================
// Helper types and functions
// ============================================================================

struct StatusMeta {
    label: &'static str,
    color: &'static str,
    cls: &'static str,
}

fn eval_status_meta(status: &str) -> StatusMeta {
    let normalized = status.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "in_progress" | "in-progress" => StatusMeta {
            label: "In Progress",
            color: "#60a5fa",
            cls: "chip-info",
        },
        "cancelling" => StatusMeta {
            label: "Cancelling",
            color: "#fbbf24",
            cls: "chip-warning",
        },
        "cancelled" => StatusMeta {
            label: "Cancelled",
            color: "#6b7280",
            cls: "chip-unknown",
        },
        "pending" => StatusMeta {
            label: "Pending",
            color: "#9ca3af",
            cls: "chip-unknown",
        },
        "complete" => StatusMeta {
            label: "Complete",
            color: "#34d399",
            cls: "chip-healthy",
        },
        "failed" => StatusMeta {
            label: "Failed",
            color: "#f87171",
            cls: "chip-critical",
        },
        _ => StatusMeta {
            label: "Unknown",
            color: "#6b7280",
            cls: "chip-unknown",
        },
    }
}

fn is_active_eval_status(status: &str) -> bool {
    let normalized = status.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "queued" | "pending" | "in_progress" | "in-progress" | "in progress" | "cancelling"
    )
}

fn format_relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let secs = (chrono::Utc::now() - dt).num_seconds();
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

/// Formats the server-reported record count without estimating missing data.
fn format_derivation_count(count: Option<i64>) -> String {
    count.map_or_else(|| "—".to_string(), |count| count.to_string())
}

#[test]
fn derivation_count_formatter_preserves_zero_and_unknown() {
    assert_eq!(format_derivation_count(None), "—");
    assert_eq!(format_derivation_count(Some(0)), "0");
    assert_eq!(format_derivation_count(Some(37)), "37");
}

fn format_eval_completed_at(item: &EvalHistoryItem) -> String {
    item.evaluation_completed_at
        .map(|dt| format_relative_time(dt))
        .unwrap_or_else(|| "—".to_string())
}

fn format_eval_duration(item: &EvalHistoryItem) -> String {
    item.evaluation_duration_ms
        .map(|ms| {
            let secs = ms / 1000;
            if secs < 60 {
                format!("{secs}s")
            } else {
                format!("{}m {}s", secs / 60, secs % 60)
            }
        })
        .unwrap_or_else(|| "—".to_string())
}

/// Determine line color based on content (matches JSX logic)
fn log_line_color(line: &str) -> &'static str {
    let lower = line.to_lowercase();
    if lower.contains("error") || lower.contains("fail") || lower.contains("✗") {
        "#f87171"
    } else if lower.contains("warn") || lower.contains("skip") {
        "#f59e0b"
    } else if lower.contains("ok")
        || lower.contains("pass")
        || lower.contains("✓")
        || lower.contains("complete")
    {
        "#34d399"
    } else {
        "inherit"
    }
}

#[cfg(test)]
mod policy_matrix_status_tests {
    use super::*;

    // ── policy_cell_category ────────────────────────────────────────────

    #[test]
    fn category_pass_is_pass() {
        assert_eq!(policy_cell_category("pass"), PolicyCellCategory::Pass);
    }

    #[test]
    fn category_fail_is_fail() {
        assert_eq!(policy_cell_category("fail"), PolicyCellCategory::Fail);
    }

    #[test]
    fn category_warn_is_warn() {
        assert_eq!(policy_cell_category("warn"), PolicyCellCategory::Warn);
    }

    #[test]
    fn category_not_assigned_is_not_assigned() {
        assert_eq!(
            policy_cell_category("not_assigned"),
            PolicyCellCategory::NotAssigned
        );
    }

    #[test]
    fn category_infrastructure_error_is_fail() {
        // Infrastructure errors must block deployment and must never be
        // silently counted as clean/passing.
        assert_eq!(
            policy_cell_category("infrastructure_error"),
            PolicyCellCategory::Fail
        );
    }

    #[test]
    fn category_nix_eval_failure_is_fail() {
        assert_eq!(
            policy_cell_category("nix_eval_failure"),
            PolicyCellCategory::Fail
        );
    }

    #[test]
    fn category_legacy_unknown_is_legacy_unknown() {
        assert_eq!(
            policy_cell_category("legacy_unknown"),
            PolicyCellCategory::LegacyUnknown
        );
    }

    #[test]
    fn category_unrecognized_status_fails_closed() {
        // A future backend status this UI doesn't know about yet must never
        // be silently treated as passing/clean.
        assert_eq!(
            policy_cell_category("some_future_status"),
            PolicyCellCategory::Fail
        );
    }

    // ── policy_cell_counts_toward_health ────────────────────────────────

    #[test]
    fn health_denominator_excludes_not_assigned_and_legacy_unknown() {
        assert!(policy_cell_counts_toward_health("pass"));
        assert!(policy_cell_counts_toward_health("fail"));
        assert!(policy_cell_counts_toward_health("warn"));
        assert!(policy_cell_counts_toward_health("infrastructure_error"));
        assert!(policy_cell_counts_toward_health("nix_eval_failure"));
        assert!(!policy_cell_counts_toward_health("not_assigned"));
        assert!(!policy_cell_counts_toward_health("legacy_unknown"));
    }

    // ── policy_cell_glyph ────────────────────────────────────────────────

    #[test]
    fn glyph_matches_expected_symbol_per_status() {
        assert_eq!(policy_cell_glyph("pass"), "✓");
        assert_eq!(policy_cell_glyph("fail"), "✗");
        assert_eq!(policy_cell_glyph("warn"), "!");
        assert_eq!(policy_cell_glyph("not_assigned"), "–");
        assert_eq!(policy_cell_glyph("legacy_unknown"), "?");
        assert_eq!(policy_cell_glyph("infrastructure_error"), "✗");
        assert_eq!(policy_cell_glyph("nix_eval_failure"), "✗");
    }

    // ── policy_cell_class_suffix ─────────────────────────────────────────

    #[test]
    fn class_suffix_is_distinct_and_stable_per_category() {
        assert_eq!(policy_cell_class_suffix("pass"), "pass");
        assert_eq!(policy_cell_class_suffix("fail"), "fail");
        assert_eq!(policy_cell_class_suffix("warn"), "warn");
        assert_eq!(policy_cell_class_suffix("not_assigned"), "not-assigned");
        assert_eq!(policy_cell_class_suffix("legacy_unknown"), "legacy-unknown");
        // infrastructure_error/nix_eval_failure must render with real (fail)
        // styling, not fall through to an unstyled/default class.
        assert_eq!(policy_cell_class_suffix("infrastructure_error"), "fail");
        assert_eq!(policy_cell_class_suffix("nix_eval_failure"), "fail");
    }
}
