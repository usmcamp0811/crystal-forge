//! Opens exact receivers only after validating server-projected attempt context.

use crate::api::models::eval_policy_matrix::*;
use crate::components::dialog_focus::{
    DialogFocusBoundary, DialogFocusRestore, DialogFocusSentinel, DialogInitialFocus,
};
use crate::components::icon::{Icon, IconName};
use crate::routes::Route;
use crate::state::evaluation_drawer::*;
use dioxus::prelude::*;

/// Validates a drawer summary against the represented matrix attempt.
pub(super) fn identity(
    snapshot: &EvalPolicyMatrixSnapshot,
    seed: &EvaluationReturn,
) -> Option<EvaluationIdentity> {
    let sha = snapshot.commit_sha.as_ref()?;
    if sha != &seed.identity.commit_sha
        || sha.len() != 40
        || !sha.bytes().all(|b| b.is_ascii_hexdigit())
        || snapshot.attempt_number != seed.identity.attempt_number
        || snapshot.evaluation_attempt_id.is_none_or(|id| {
            id.is_nil() || (!seed.identity.attempt_id.is_nil() && id != seed.identity.attempt_id)
        })
    {
        return None;
    }
    Some(EvaluationIdentity {
        commit_id: seed.identity.commit_id,
        commit_sha: sha.clone(),
        attempt_id: snapshot.evaluation_attempt_id?,
        attempt_number: snapshot.attempt_number,
    })
}

/// Opens Config at the captured system UUID and full commit SHA.
#[component]
pub(super) fn OpenSystem(
    snapshot: EvalPolicyMatrixSnapshot,
    system_id: Option<uuid::Uuid>,
) -> Element {
    let seed = use_context::<Signal<EvaluationReturn>>();
    let exact = identity(&snapshot, &seed.read());
    let nav = navigator();
    rsx! { button { class: "btn btn-ghost xs focus-ring", disabled: exact.is_none() || system_id.is_none(),
    id: "evaluation-open-system-{system_id:?}",
    title: "Inspect this system at the exact evaluated commit",
    onclick: move |_| {
        let (Some(exact), Some(system_id)) = (exact.clone(), system_id) else { return; };
        let mut saved = seed(); saved.identity = exact.clone(); saved.system = Some(system_id);
        spawn(async move {
            saved.dom = capture_dom().await;
            *EVALUATION_RETURN.write() = Some(saved);
            nav.push(Route::SystemDetailView { id: system_id.to_string(), tab: "config".into(), poam: String::new(), config_mode: "commit".into(), revision: exact.commit_sha, generation: String::new(), deploy_generation: String::new(), cve_target: String::new(), cve_mode: String::new() });
        });
    }, "Open system" } }
}

/// Offers one exact evidence action per captured contributing bundle.
#[component]
pub(super) fn OpenEvidence(
    snapshot: EvalPolicyMatrixSnapshot,
    system_id: Option<uuid::Uuid>,
    policy: EvalPolicyColumn,
    bundle: EvalBundleColumn,
) -> Element {
    let seed = use_context::<Signal<EvaluationReturn>>();
    let exact = identity(&snapshot, &seed.read());
    let nav = navigator();
    rsx! { button { class: "btn btn-ghost xs focus-ring", disabled: exact.is_none() || system_id.is_none(),
    id: "evaluation-evidence-{system_id:?}-{policy.policy_version_id}-{bundle.bundle_version_id}",
    onclick: move |_| {
        let (Some(evaluation), Some(system_id)) = (exact.clone(), system_id) else { return; };
        let evidence = EvaluationEvidence { evaluation: evaluation.clone(), system_id, bundle_id: bundle.bundle_id, bundle_version_id: bundle.bundle_version_id, policy_id: policy.policy_id, policy_version_id: policy.policy_version_id };
        let mut saved = seed(); saved.identity = evaluation; saved.system = None;
        spawn(async move {
            saved.dom = capture_dom().await;
            *EVALUATION_RETURN.write() = Some(saved);
            *EVALUATION_EVIDENCE.write() = Some(evidence.clone());
            nav.push(Route::ComplianceEvaluationView { commit_id: evidence.evaluation.commit_id, attempt_id: evidence.evaluation.attempt_id, bundle: bundle.bundle_id.to_string(), version: bundle.bundle_version_id.to_string(), system: system_id.to_string(), policy: policy.policy_version_id.to_string() });
        });
    }, "View in compliance evidence · {bundle.name} · {bundle.version}" } }
}

/// Supplies accessible descriptions for the visibly unavailable retained tabs.
/// Disabled labels and titles expose the state without adding a diagnostic panel.
#[component]
pub(super) fn RetainedTabReasons() -> Element {
    rsx! {
        div { class: "sr-only",
            p { id: "retained-log-unavailable", "Log unavailable for retained attempt. An exact-attempt log API is not available." }
            p { id: "retained-graph-unavailable", "Dependency graph unavailable for retained attempt. An exact-attempt graph API is not available." }
        }
    }
}

/// Opens a direct historical URL without manufacturing a latest summary.
/// Pinned identity is revalidated before rendering context or requesting evidence.
/// Missing metadata stays unavailable; captured row names are not configuration
/// inventory counts. Only the pinned inventory and summary supply header counts.
#[component]
pub(super) fn RetainedAttemptDrawer(
    saved: EvaluationReturn,
    on_close: EventHandler<()>,
) -> Element {
    let nav = navigator();
    let mut navigation_focus = use_context::<Signal<Option<super::NavigationFocus>>>();
    let identity = saved.identity.clone();
    let request = identity.clone();
    let mut displayed = use_signal(|| None);
    let _resource = use_resource(move || {
        let request = request.clone();
        async move {
            loop {
                let result = crate::api::client::fetch_eval_attempt_policy_matrix(
                    request.commit_id,
                    request.attempt_id,
                )
                .await;
                let terminal = result.as_ref().is_ok_and(|data| {
                    data.matrix
                        .as_ref()
                        .and_then(|snapshot| snapshot.attempt_status.as_deref())
                        .is_some_and(|status| matches!(status, "complete" | "failed" | "cancelled"))
                });
                // Publish each response rather than waiting for the polling
                // future to finish. Queued retained attempts must discover their
                // own inventory, never a newer retry's summary or cached count.
                displayed.set(Some(result));
                if terminal {
                    break;
                }
                #[cfg(target_arch = "wasm32")]
                gloo_timers::future::TimeoutFuture::new(5_000).await;
                #[cfg(not(target_arch = "wasm32"))]
                break;
            }
        }
    });
    let maximized = use_signal(|| saved.maximized);
    let mut context = use_context_provider(|| Signal::new(saved.clone()));
    let seed = saved.clone();
    use_effect(move || {
        let mut next = seed.clone();
        next.maximized = maximized();
        if *context.peek() != next {
            context.set(next);
        }
    });
    let displayed_value = displayed.read();
    let header_snapshot = displayed_value
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .filter(|data| identity.matches_response(data))
        .and_then(|data| data.matrix.as_ref());
    let status = header_snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.attempt_status.as_deref());
    let status_meta = super::eval_status_meta(status.unwrap_or("unavailable"));
    let fallback_name = match &saved.target {
        Some(EvaluationTarget::Queue(item)) => item.flake_name.clone(),
        Some(EvaluationTarget::History(item)) => item.flake_name.clone(),
        None => format!("Retained commit {}", identity.commit_id),
    };
    let source_label = header_snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.source_label.as_ref());
    let flake_name = source_label
        .map(|source| source.name.as_str())
        .unwrap_or(&fallback_name);
    let branch = source_label
        .and_then(|source| source.branch.as_deref())
        .or_else(|| match &saved.target {
            Some(EvaluationTarget::Queue(item)) => Some(item.branch.as_str()),
            Some(EvaluationTarget::History(item)) => Some(item.branch.as_str()),
            None => None,
        });
    rsx! {
        div { class: "fl-tray-backdrop", onclick: move |_| on_close.call(()) }
        aside { id: "retained-evaluation-dialog", class: if maximized() { "fl-tray ed-drawer fl-tray-max" } else { "fl-tray ed-drawer" }, role: "dialog", aria_modal: "true", aria_label: "Evaluation detail", "data-attempt-number": "{identity.attempt_number}", "data-attempt-id": "{identity.attempt_id}", "data-commit-sha": "{identity.commit_sha}", tabindex: "-1",
            onkeydown: move |event| if event.key() == Key::Escape { event.stop_propagation(); on_close.call(()); },
            DialogFocusRestore {}
            DialogInitialFocus { dialog_id: "retained-evaluation-dialog" }
            DialogFocusSentinel { dialog_id: "retained-evaluation-dialog".to_string(), boundary: DialogFocusBoundary::Last }
            header { class: "fl-tray-head",
                div { style: "display:flex;align-items:center;gap:12px;min-width:0;flex:1;",
                    span { class: "ed-identity-icon", Icon { name: IconName::Evaluation, size: 18 } }
                    div { style: "min-width:0;",
                        h2 { id: "retained-evaluation-title", class: "ed-identity-title",
                            span { class: "ed-flake-name", "{flake_name}" }
                            span { class: "chip chip-unknown", style: "font-size:10px;", {branch.unwrap_or("branch unavailable")} }
                            span { class: "chip {status_meta.cls}",
                                span { class: "chip-dot", style: "background: {status_meta.color};" }
                                if status.is_some() { "{status_meta.label}" } else { "Unavailable" }
                            }
                        }
                        div { class: "mono ed-identity-meta",
                            "{identity.commit_sha} · {identity.commit_id}"
                            span { class: "chip chip-unknown", style: "font-size: 10px; margin-left: 6px;", title: "Exact retained evaluation attempt", "attempt {identity.attempt_number}" }
                        }
                    }
                }
                div { style: "display:flex;gap:6px;align-items:center;",
                    super::drawer_header::ExpandButton { maximized }
                    button { class: "btn-icon focus-ring", aria_label: "Close", onclick: move |_| on_close.call(()), Icon { name: IconName::X, size: 16 } }
                }
            }
            match &*displayed_value {
                None => rsx! { p { role: "status", "Loading exact retained attempt…" } },
                Some(Ok(data)) if identity.matches_response(data) => {
                    let snapshot = data.matrix.as_ref();
                    let inventory = snapshot.and_then(|snapshot| snapshot.inventory.as_ref());
                    let summary = inventory.and(snapshot.and_then(|snapshot| snapshot.configuration_summary.as_ref()));
                    let completed = snapshot.and_then(|snapshot| snapshot.completed_at);
                    let started = snapshot.and_then(|snapshot| snapshot.started_at);
                    let timestamp = completed.or(started);
                    let duration = started.zip(completed)
                        .filter(|(start, end)| end >= start)
                        .map(|(start, end)| (end - start).num_seconds())
                        .map(|seconds| if seconds < 60 { format!("{seconds}s") } else { format!("{}m {}s", seconds / 60, seconds % 60) })
                        .unwrap_or_else(|| "—".into());
                    let live = snapshot.and_then(|snapshot| snapshot.attempt_status.as_deref()).is_some_and(super::is_active_eval_status);
                    let total = snapshot.map_or(0, super::snapshot_population);
                    rsx! {
                        div { class: "ed-stats",
                            div { class: "ed-stat", div { class: "ed-stat-label", if completed.is_some() { "Completed" } else { "Started" } } div { class: "ed-stat-val ed-stat-time", if let Some(timestamp) = timestamp { super::drawer_header::Timestamp { timestamp } } else { "—" } } }
                            div { class: "ed-stat", div { class: "ed-stat-label", "Duration" } div { class: "ed-stat-val mono", "{duration}" } }
                            div { class: "ed-stat", title: "Visible planned and discovered configurations for this exact attempt.", div { class: "ed-stat-label", "Systems" } super::InventoryCount { available: inventory.is_some(), count: total, live } }
                            div { class: "ed-stat", div { class: "ed-stat-label", "Policy" } super::DrawerPolicyCounts { available: summary.is_some(), passed: summary.map_or(0, |summary| summary.passed) as i64, failed: summary.map_or(0, |summary| summary.failed) as i64, pending: summary.map_or(0, |summary| summary.pending) as i64, unavailable: summary.map_or(0, |summary| summary.unavailable) as i64, evaluation_errors: summary.map_or(0, |summary| summary.evaluation_failed) as i64, excluded: summary.map_or(0, |summary| summary.excluded) as i64, live } }
                            div { class: "ed-stat", title: "Exact-attempt derivation counts are unavailable in this response.", div { class: "ed-stat-label", "Derivations" } div { class: "ed-stat-val", "—" } }
                        }
                        div { class: "sd-tabs", role: "tablist", aria_label: "Retained evaluation evidence",
                            button { class: "sd-tab", role: "tab", disabled: true, aria_disabled: "true", aria_describedby: "retained-log-unavailable", title: "No exact-attempt log API is available.", "Log unavailable for retained attempt" }
                            button { class: "sd-tab active", role: "tab", aria_selected: "true", "Policy matrix" }
                            button { class: "sd-tab", role: "tab", disabled: true, aria_disabled: "true", aria_describedby: "retained-graph-unavailable", title: "No exact-attempt graph API is available.", "Dependency graph unavailable" }
                        }
                        div { class: "ed-body",
                            RetainedTabReasons {}
                            super::EvalDrawerPolicyTab {
                                key: "{identity.commit_id}-{identity.attempt_id}",
                                commit_id: identity.commit_id,
                                live,
                                on_open_policy: move |policy_name: String| {
                                    navigation_focus.set(Some(super::NavigationFocus { target: super::FocusTarget::Policies, commit_sha: None, flake_name: None, status: None, policy_name: Some(policy_name) }));
                                    nav.push(Route::PoliciesView {});
                                },
                            }
                        }
                    }
                },
                _ => rsx! { p { role: "alert", "Exact retained attempt unavailable or unauthorized. No latest-attempt fallback was used." } },
            }
            DialogFocusSentinel { dialog_id: "retained-evaluation-dialog".to_string(), boundary: DialogFocusBoundary::First }
        }
    }
}
