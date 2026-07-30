//! Evaluations view - rebuilt to match JSX mockup design exactly.

use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use gloo_timers::future::TimeoutFuture;

use crate::alerts::{
    NAV_BADGES, acknowledge_with_cursor_and_ids_async, attention_row_class, dismiss_attention_item,
    occurrence_id_for_subject, occurrence_ids_for_rendered_subjects, should_flash,
};

use crate::api::{
    client::{
        ApiClientError, cancel_commit_evaluation, fetch_eval_dependency_graph, fetch_eval_history,
        fetch_eval_policy_matrix, fetch_eval_queue, force_cancel_commit_evaluation,
        re_evaluate_commit, reorder_eval_queue,
    },
    models::{EvalHistoryItem, EvalHistoryPage, EvalQueueItem},
};
use crate::components::{Icon, IconName};
use crate::hooks::{InfiniteScroll, use_infinite_scroll};
use crate::routes::Route;
use crate::state::navigation_focus::{FocusTarget, NavigationFocus};
use crate::views::latest_filter::{
    LatestFilterState, marker_matches, replace_unique_by, request_state, reset_key, retain_visible,
};

const FETCH_LIMIT_MAX: i64 = 10_000; // must match backend LIMIT_MAX

#[derive(Clone, Copy, PartialEq, Eq)]
enum EvaluationsTab {
    ActiveQueue,
    History,
}

#[derive(Clone, PartialEq)]
enum EvalDrawerTarget {
    Queue(EvalQueueItem),
    History(EvalHistoryItem),
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

fn history_item_matches_filters(
    item: &EvalHistoryItem,
    status: &str,
    flake: &str,
    search: &str,
) -> bool {
    (status == "all" || item.evaluation_status == status)
        && (flake == "all"
            || item
                .flake_name
                .to_lowercase()
                .contains(&flake.to_lowercase()))
        && (search.is_empty()
            || [
                item.flake_name.as_str(),
                item.branch.as_str(),
                item.commit_hash.as_str(),
                item.commit_message.as_deref().unwrap_or_default(),
                item.author.as_deref().unwrap_or_default(),
                item.evaluation_status.as_str(),
            ]
            .into_iter()
            .any(|value| value.to_lowercase().contains(search)))
}

#[component]
pub fn EvaluationsView() -> Element {
    rsx! { EvaluationsPage {} }
}

#[component]
pub fn EvaluationsCommitView(commit_id: i32) -> Element {
    let _ = commit_id;
    rsx! { EvaluationsPage {} }
}

#[component]
fn EvaluationsPage() -> Element {
    let nav = navigator();
    let mut navigation_focus = use_context::<Signal<Option<NavigationFocus>>>();
    let mut queue_items = use_signal(Vec::<EvalQueueItem>::new);
    let mut active_fetch_limit = use_signal(|| 200_i64);
    let mut active_refresh = use_signal(|| 0_u64);
    let mut history_refresh = use_signal(|| 0_u64);
    let mut active_tab = use_signal(|| EvaluationsTab::ActiveQueue);
    let mut latest_filter = use_signal(LatestFilterState::default);
    let mut search_query = use_signal(String::new);
    let mut drawer_target = use_signal(|| None::<EvalDrawerTarget>);
    let mut history_selected_ids = use_signal(std::collections::HashSet::<i32>::new);
    // Keyboard navigation: index into the currently visible list (queue or history).
    // Start at 0 so the first row is auto-selected when the page loads (matching JSX).
    let mut focused_index: Signal<Option<usize>> = use_signal(|| None);

    // Active queue multi-select (bulk cancel)
    let mut active_selected_ids = use_signal(std::collections::HashSet::<i32>::new);

    // Toast state for soft-cancel undo
    let mut toast_msg = use_signal(|| None::<String>);

    // History tab state
    let mut history_fetch_limit = use_signal(|| 50_i64);
    let mut history_status_filter = use_signal(|| String::from("all"));
    let mut history_flake_filter = use_signal(|| String::from("all"));
    let mut history_select_all_loaded = use_signal(|| true);
    let mut history_ack_cursor = use_signal(|| None::<String>);
    let mut evals_ack_sent = use_signal(|| false);
    // Accumulated history items across the current page-1 fetch window.
    let mut history_items_acc: Signal<Vec<EvalHistoryItem>> = use_signal(Vec::new);
    let mut history_total_acc = use_signal(|| 0_i64);
    let mut history_domain_total = use_signal(|| 0_i64);

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

    let history_resource = use_resource(move || async move {
        let _ = history_refresh();
        let limit = history_fetch_limit();
        let status = history_status_filter();
        let flake = history_flake_filter();
        let search = search_query();
        let (search, latest_only) = request_state(&search, latest_filter());
        fetch_eval_history(
            1,
            limit,
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
            search.as_deref(),
            latest_only,
        )
        .await
    });

    let queue_resource = use_resource(move || async move {
        let _ = active_refresh();
        let search = search_query();
        let (search, latest_only) = request_state(&search, latest_filter());
        fetch_eval_queue(active_fetch_limit(), search.as_deref(), latest_only).await
    });

    {
        let mut active_refresh = active_refresh.clone();
        let mut history_refresh = history_refresh.clone();
        let mut active_tab = active_tab;
        use_future(move || async move {
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

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = (active_refresh, history_refresh);
                    break;
                }
            }
        });
    }

    use_effect(move || {
        if let Some(Ok(summary)) = &*queue_resource.read() {
            queue_items.set(replace_unique_by(summary.items.clone(), |item| {
                item.commit_id
            }));
        }
    });

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

    // Replace the loaded history window atomically on each refresh. Fetching
    // from page 1 with a growing limit avoids inconsistencies from mutable
    // offset pagination (review finding #1).
    use_effect(move || {
        if let Some(Ok(page_data)) = &*history_resource.read() {
            history_total_acc.set(page_data.total_count);
            history_domain_total.set(page_data.domain_total);
            history_items_acc.set(replace_unique_by(page_data.items.clone(), |item| {
                item.commit_id
            }));
            history_ack_cursor.set(NAV_BADGES.read_unchecked().observed_at.clone());
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

    use_effect(move || {
        if let Some(Ok(page_data)) = &*history_resource.read() {
            history_ack_cursor.set(NAV_BADGES.read_unchecked().observed_at.clone());
            if history_select_all_loaded() {
                let search = search_query().trim().to_lowercase();
                let status = history_status_filter();
                let flake = history_flake_filter();
                let ids = page_data
                    .items
                    .iter()
                    .filter(|item| history_item_matches_filters(item, &status, &flake, &search))
                    .filter(|item| {
                        marker_matches(latest_filter().enabled(), item.is_latest_per_flake)
                    })
                    .take(hist_paging.count())
                    .map(|item| item.commit_id)
                    .collect::<std::collections::HashSet<_>>();
                history_selected_ids.set(ids);
            }
        }
    });

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
        let visible = queue_items
            .read()
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

    use_effect(move || {
        let visible = history_items_acc
            .read()
            .iter()
            .filter(|item| {
                history_item_matches_filters(
                    item,
                    &history_status_filter(),
                    &history_flake_filter(),
                    &search_query().trim().to_lowercase(),
                )
            })
            .filter(|item| marker_matches(latest_filter().enabled(), item.is_latest_per_flake))
            .take(hist_paging.count())
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
    let history_visible_rows = history_items_acc
        .read()
        .iter()
        .filter(|item| {
            history_item_matches_filters(
                item,
                &history_status_filter(),
                &history_flake_filter(),
                &search_query().trim().to_lowercase(),
            )
        })
        .filter(|item| marker_matches(latest_filter().enabled(), item.is_latest_per_flake))
        .take(hist_paging.count())
        .cloned()
        .collect::<Vec<_>>();
    let selected_history_rows = {
        let ids = history_selected_ids.read();
        history_visible_rows
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
                    // No keyboard nav while drawer is open
                    if drawer_target.read().is_some() {
                        if evt.key() == Key::Escape {
                            drawer_target.set(None);
                        }
                        return;
                    }

                    // Keyboard nav is bounded to the currently *rendered* slice so
                    // focus never lands on an invisible row (review finding #4).
                    // `hist_paging` is created in the parent so the visible count
                    // is directly accessible here, but it may briefly exceed the
                    // number of actually loaded rows while a server page is in
                    // flight — cap to the accumulated items count (finding #4).
                    let list_len = if active_tab() == EvaluationsTab::ActiveQueue {
                        paged_active_items.len()
                    } else {
                        hist_paging.count().min(history_items_acc.read().len())
                    };

                    match evt.key() {
                        Key::Character(ref c) if c == "j" || c == "ArrowDown" => {
                            let next = match focused_index() {
                                None => 0,
                                Some(i) => (i + 1).min(list_len.saturating_sub(1)),
                            };
                            focused_index.set(Some(next));
                        }
                        Key::ArrowDown => {
                            let next = match focused_index() {
                                None => 0,
                                Some(i) => (i + 1).min(list_len.saturating_sub(1)),
                            };
                            focused_index.set(Some(next));
                        }
                        Key::Character(ref c) if c == "k" => {
                            let next = match focused_index() {
                                None => 0,
                                Some(0) => 0,
                                Some(i) => i - 1,
                            };
                            focused_index.set(Some(next));
                        }
                        Key::ArrowUp => {
                            let next = match focused_index() {
                                None => 0,
                                Some(0) => 0,
                                Some(i) => i - 1,
                            };
                            focused_index.set(Some(next));
                        }
                        Key::Enter => {
                            if let Some(idx) = focused_index() {
                                if active_tab() == EvaluationsTab::ActiveQueue {
                                    // Use paged slice — idx is clamped to its length above.
                                    if let Some(ev) = paged_active_items.get(idx) {
                                        drawer_target.set(Some(EvalDrawerTarget::Queue(ev.clone())));
                                    }
                                } else {
                                    // Use accumulated history items.
                                    let item = history_items_acc.read().get(idx).cloned();
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
                            "History"
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
                                placeholder: if active_tab() == EvaluationsTab::ActiveQueue { "Search queue…" } else { "Search history…" },
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
                                        let mut select_all_sig = history_select_all_loaded.clone();
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
                                                // Disable auto-select so the next poll doesn't repopulate
                                                // the selection with successful rows (review finding #3).
                                                select_all_sig.set(false);
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
                            history_resource: history_resource,
                            history_status_filter: history_status_filter,
                            history_flake_filter: history_flake_filter,
                            history_fetch_limit: history_fetch_limit,
                            history_select_all_loaded: history_select_all_loaded,
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

                if let Some(target) = drawer_target.read().clone() {
                    EvalDrawer {
                        target: target,
                        refresh: active_refresh,
                        on_close: move |_| drawer_target.set(None),
                        toast_msg: toast_msg,
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
    queue_items: Signal<Vec<EvalQueueItem>>,
    mut drawer_target: Signal<Option<EvalDrawerTarget>>,
    focused_index: Signal<Option<usize>>,
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
                                td { style: "font-size: 12px; color: var(--cf-text-secondary);", "{ev_clone.system_count} hosts" }
                                td {
                                    div { style: "display: flex; gap: 6px;",
                                        span { class: "chip chip-healthy", "{ev_clone.passed_count} ✓" }
                                        if ev_clone.policy_failed_count > 0 {
                                            span { class: "chip chip-critical", "{ev_clone.policy_failed_count} ✗" }
                                        }
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
    history_resource: Resource<Result<EvalHistoryPage, ApiClientError>>,
    mut history_status_filter: Signal<String>,
    mut history_flake_filter: Signal<String>,
    mut history_fetch_limit: Signal<i64>,
    mut history_select_all_loaded: Signal<bool>,
    mut refresh: Signal<u64>,
    mut history_selected_ids: Signal<std::collections::HashSet<i32>>,
    mut drawer_target: Signal<Option<EvalDrawerTarget>>,
    focused_index: Signal<Option<usize>>,
    flash_evals: bool,
    history_items_acc: Signal<Vec<EvalHistoryItem>>,
    history_total_acc: Signal<i64>,
    history_domain_total: Signal<i64>,
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
                                        history_select_all_loaded.set(true);
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
                        history_select_all_loaded.set(true);
                        refresh.set(refresh() + 1);
                    },
                    option { value: "all", "All flakes" }
                    if let Some(Ok(page_data)) = &*history_snapshot {
                        {
                            let mut flakes: Vec<String> = page_data
                                .items
                                .iter()
                                .map(|item| item.flake_name.clone())
                                .collect();
                            flakes.sort();
                            flakes.dedup();
                            flakes.into_iter().map(|flake_name| rsx! {
                                option {
                                    key: "{flake_name}",
                                    value: "{flake_name}",
                                    "{flake_name}"
                                }
                            })
                        }
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
                        let all_loaded = history_items_acc
                            .read()
                            .iter()
                            .filter(|item| history_item_matches_filters(
                                item,
                                &history_status_filter(),
                                &history_flake_filter(),
                                &search_query().trim().to_lowercase(),
                            ))
                            .filter(|item| marker_matches(latest_filter().enabled(), item.is_latest_per_flake))
                            .cloned()
                            .collect::<Vec<_>>();
                        let paged_items: Vec<EvalHistoryItem> = all_loaded.iter().take(hist_paging.count()).cloned().collect();
                        let hist_has_more = hist_paging.count() < all_loaded.len() || hist_server_has_more;
                        // Select-all operates on all loaded rows (not just the visible page)
                        // so check/uncheck is symmetric and consistent (fixes review finding #5).
                        let all_loaded_ids: Vec<i32> = paged_items.iter().map(|item| item.commit_id).collect();
                        let all_checked = !all_loaded_ids.is_empty()
                            && all_loaded_ids.iter().all(|id| history_selected_ids.read().contains(id));
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
                                        if is_focused { "kbd-focused" } else { "" },
                                        "evals",
                                        &eval_key,
                                        is_failed,
                                        is_failed && flash_evals,
                                    );

                                    rsx! {
                                        tr {
                                            key: "{commit_id}",
                                            class: "{row_class}",
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
                                                    history_select_all_loaded.set(false);
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
                                                "{ev.system_count}"
                                            }
                                            td {
                                                div {
                                                    style: "display: flex; gap: 6px;",
                                                    span {
                                                        class: "chip chip-healthy",
                                                        style: "font-size: 10px;",
                                                        "{ev.passed_count} ✓"
                                                    }
                                                    if ev.policy_failed_count > 0 {
                                                        span {
                                                            class: "chip chip-critical",
                                                            style: "font-size: 10px;",
                                                            "{ev.policy_failed_count} ✗"
                                                        }
                                                    }
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
                                                    // Re-evaluate
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
                                                    // Retry (failed only)
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
    on_close: EventHandler<MouseEvent>,
    mut toast_msg: Signal<Option<String>>,
    on_open_policy: EventHandler<String>,
) -> Element {
    let mut drawer_tab = use_signal(|| String::from("log"));

    match target {
        EvalDrawerTarget::Queue(ev) => {
            let status_meta = eval_status_meta(&ev.evaluation_status);
            let can_cancel = matches!(ev.evaluation_status.as_str(), "pending" | "in_progress");
            let can_force_cancel = ev.evaluation_status == "cancelling";
            let is_live = can_cancel || can_force_cancel;
            let close_click = move |evt: MouseEvent| on_close.call(evt);

            rsx! {
                div {
                    class: "fl-tray-backdrop",
                    onclick: close_click,
                }
                aside {
                    class: "fl-tray",
                    role: "dialog",
                    "aria-label": "Evaluation detail",
                    header {
                        class: "fl-tray-head",
                        div {
                            style: "display: flex; align-items: center; gap: 12px; min-width: 0; flex: 1;",
                            Icon { name: IconName::Terminal, size: 18 }
                            div { style: "min-width: 0;",
                            h2 {
                                "{ev.flake_name}"
                                span { class: "chip chip-unknown", style: "font-size: 10px;", "{ev.branch}" }
                                span {
                                    class: "chip {status_meta.cls}",
                                    span { class: "chip-dot", style: "background: {status_meta.color};" }
                                    "{status_meta.label}"
                                    if is_live { span { class: "ed-pulse" } }
                                }
                            }
                            div {
                                class: "fqdn",
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
                                    onclick: move |_| {
                                        let mut refresh_sig = refresh.clone();
                                        let mut toast = toast_msg.clone();
                                        let commit_id = ev.commit_id;
                                        spawn(async move {
                                            if let Err(e) = force_cancel_commit_evaluation(commit_id).await {
                                                toast.set(Some(format!("Force cancel failed: {}", e)));
                                            } else {
                                                refresh_sig.set(refresh_sig() + 1);
                                            }
                                        });
                                    },
                                    "Force-cancel"
                                }
                            }
                            button {
                                class: "btn-icon focus-ring",
                                onclick: close_click,
                                title: "Close",
                                Icon { name: IconName::X, size: 16 }
                            }
                        }
                    }
                    div {
                        class: "ed-stats",
                        div { class: "ed-stat", div { class: "ed-stat-label", "Started" } div { class: "ed-stat-val", style: "font-size: 13px; font-weight: 600;", "{format_relative_time(ev.committed_at)}" } }
                        div { class: "ed-stat", div { class: "ed-stat-label", "Duration" } div { class: "ed-stat-val mono", "—" } }
                        div { class: "ed-stat", div { class: "ed-stat-label", "Systems" } div { class: "ed-stat-val", "{ev.system_count}" } }
                        div {
                            class: "ed-stat",
                            div { class: "ed-stat-label", "Policy" }
                            div {
                                class: "ed-stat-val",
                                style: "display: flex; gap: 6px; align-items: baseline;",
                                span { style: "color: #34d399;", "{ev.passed_count}" }
                                span { style: "font-size: 12px; color: var(--cf-text-muted);", "/" }
                                span {
                                    style: if ev.policy_failed_count > 0 { "color: #f87171;" } else { "color: var(--cf-text-muted);" },
                                    "{ev.policy_failed_count}"
                                }
                            }
                        }
                        div { class: "ed-stat", div { class: "ed-stat-label", "Derivations" } div { class: "ed-stat-val", "{ev.system_count * 18}" } }
                    }
                    div {
                        class: "sd-tabs",
                        style: "padding: 0 16px; border-bottom: 1px solid var(--cf-card-border); flex-shrink: 0;",
                        button {
                            class: if drawer_tab() == "log" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            onclick: move |_| drawer_tab.set("log".to_string()),
                            Icon { name: IconName::Terminal, size: 12 }
                            " Log"
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
                            onclick: move |_| drawer_tab.set("graph".to_string()),
                            Icon { name: IconName::Git, size: 12 }
                            " Dependency graph"
                        }
                    }
                    div {
                        class: "ed-body",
                        if drawer_tab() == "log" {
                            EvalDrawerLogTabQueue { ev: ev.clone(), live: is_live }
                        } else if drawer_tab() == "policy" {
                            EvalDrawerPolicyTab {
                                commit_id: ev.commit_id,
                                on_open_policy: on_open_policy,
                            }
                        } else {
                            EvalDrawerGraphTab {
                                commit_id: ev.commit_id,
                            }
                        }
                    }
                }
            }
        }
        EvalDrawerTarget::History(ev) => {
            let status_meta = eval_status_meta(&ev.evaluation_status);
            let is_live = matches!(
                ev.evaluation_status.as_str(),
                "pending" | "in_progress" | "cancelling"
            );
            let close_click = move |evt: MouseEvent| on_close.call(evt);

            rsx! {
                div {
                    class: "fl-tray-backdrop",
                    onclick: close_click,
                }
                aside {
                    class: "fl-tray",
                    role: "dialog",
                    "aria-label": "Evaluation detail",
                    header {
                        class: "fl-tray-head",
                        div {
                            style: "display: flex; align-items: center; gap: 12px; min-width: 0; flex: 1;",
                            Icon { name: IconName::Terminal, size: 18 }
                            div { style: "min-width: 0;",
                            h2 {
                                "{ev.flake_name}"
                                span { class: "chip chip-unknown", style: "font-size: 10px;", "{ev.branch}" }
                                span {
                                    class: "chip {status_meta.cls}",
                                    span { class: "chip-dot", style: "background: {status_meta.color};" }
                                    "{status_meta.label}"
                                    if is_live { span { class: "ed-pulse" } }
                                }
                            }
                            div {
                                class: "fqdn",
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
                        button {
                            class: "btn-icon focus-ring",
                            onclick: close_click,
                            title: "Close",
                            Icon { name: IconName::X, size: 16 }
                        }
                    }
                    div {
                        class: "ed-stats",
                        div { class: "ed-stat", div { class: "ed-stat-label", "Started" } div { class: "ed-stat-val", style: "font-size: 13px; font-weight: 600;", "{format_relative_time(ev.committed_at)}" } }
                        div { class: "ed-stat", div { class: "ed-stat-label", "Duration" } div { class: "ed-stat-val mono", "{format_eval_duration(&ev)}" } }
                        div { class: "ed-stat", div { class: "ed-stat-label", "Systems" } div { class: "ed-stat-val", "{ev.system_count}" } }
                        div {
                            class: "ed-stat",
                            div { class: "ed-stat-label", "Policy" }
                            div {
                                class: "ed-stat-val",
                                style: "display: flex; gap: 6px; align-items: baseline;",
                                span { style: "color: #34d399;", "{ev.passed_count}" }
                                span { style: "font-size: 12px; color: var(--cf-text-muted);", "/" }
                                span {
                                    style: if ev.policy_failed_count > 0 { "color: #f87171;" } else { "color: var(--cf-text-muted);" },
                                    "{ev.policy_failed_count}"
                                }
                            }
                        }
                        div { class: "ed-stat", div { class: "ed-stat-label", "Derivations" } div { class: "ed-stat-val", "{ev.system_count * 18}" } }
                    }
                    div {
                        class: "sd-tabs",
                        style: "padding: 0 16px; border-bottom: 1px solid var(--cf-card-border); flex-shrink: 0;",
                        button {
                            class: if drawer_tab() == "log" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            onclick: move |_| drawer_tab.set("log".to_string()),
                            Icon { name: IconName::Terminal, size: 12 }
                            " Log"
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
                            onclick: move |_| drawer_tab.set("graph".to_string()),
                            Icon { name: IconName::Git, size: 12 }
                            " Dependency graph"
                        }
                    }
                    div {
                        class: "ed-body",
                        if drawer_tab() == "log" {
                            EvalDrawerLogTabHistory { ev: ev.clone(), live: is_live }
                        } else if drawer_tab() == "policy" {
                            EvalDrawerPolicyTab {
                                commit_id: ev.commit_id,
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
                        if ev.evaluation_status != "complete" {
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
                #[cfg(not(target_arch = "wasm32"))]
                break;
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
                #[cfg(not(target_arch = "wasm32"))]
                break;
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

#[component]
fn EvalDrawerPolicyTab(commit_id: i32, on_open_policy: EventHandler<String>) -> Element {
    let policy_resource =
        use_resource(move || async move { fetch_eval_policy_matrix(commit_id).await });
    let policy_snapshot = policy_resource.read();

    // Interactive state: filter, sort, expanded row, policy filter (matches JSX)
    let mut filter_state = use_signal(|| "all".to_string());
    let mut sort_state = use_signal(|| "health".to_string());
    let mut expanded = use_signal(|| None::<String>);
    let mut open_cause = use_signal(|| None::<String>);
    let mut policy_filter = use_signal(|| None::<String>);

    rsx! {
        div {
            style: "flex: 1; overflow: hidden; display: flex; flex-direction: column;",
            match &*policy_snapshot {
                None => rsx! {
                    div { style: "color: var(--cf-text-muted); font-size: 12px; padding: 14px;", "Loading policy matrix..." }
                },
                Some(Err(_)) => rsx! {
                    div { style: "color: #f87171; font-size: 12px; padding: 14px;", "Failed to load policy matrix" }
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
                            "clean" => result.retain(|r| r.fail == 0 && r.warn == 0 && r.legacy_unknown == 0),
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
                    let count_clean = annotated.iter().filter(|r| r.fail == 0 && r.warn == 0 && r.legacy_unknown == 0).count();

                    let cell_glyph = policy_cell_glyph;

                    rsx! {
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
                                    let counts = format!("{}+{}+{}", iss.fail, iss.warn, iss.pass);
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
                                                                    div { style: "width: {(row.fail as f64 / policies.len() as f64) * 100.0}%; background: #f87171;" }
                                                                }
                                                                if row.warn > 0 {
                                                                    div { style: "width: {(row.warn as f64 / policies.len() as f64) * 100.0}%; background: #f59e0b;" }
                                                                }
                                                                if row.pass > 0 {
                                                                    div { style: "width: {(row.pass as f64 / policies.len() as f64) * 100.0}%; background: #34d399;" }
                                                                }
                                                            }
                                                            span { class: "mono pm-health-num", style: "color: {health_color};",
                                                                "{row.pass}/{row.evaluated}"
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

                                                                        if row.fail == 0 && row.warn == 0 && row.legacy_unknown == 0 {
                                                                            div { style: "font-size: 12px; color: #34d399; display: flex; align-items: center; gap: 8px;",
                                                                                Icon { name: IconName::Check, size: 14 }
                                                                                " All policies pass for this system."
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
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                    if has_pending_counts() && graph_pending_polls() < GRAPH_PENDING_POLL_MAX {
                        graph_pending_polls.set(graph_pending_polls() + 1);
                        graph_refresh.set(graph_refresh() + 1);
                    }
                }

                #[cfg(not(target_arch = "wasm32"))]
                break;
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
                            div { style: "color: var(--cf-text-muted); font-size: 12px;", "No systems recorded for this commit yet." }
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
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    gloo_timers::future::TimeoutFuture::new(1000).await;
                    secs.set((secs() + 1) % 6);
                }
                #[cfg(not(target_arch = "wasm32"))]
                break;
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
        "pending" | "in_progress" | "in-progress" | "in progress" | "cancelling"
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
