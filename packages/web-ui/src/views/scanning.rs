use dioxus::prelude::*;

use crate::api::client::{
    fetch_scanning_activity, fetch_scanning_queue, fetch_scanning_schedule, fetch_scanning_stats,
    fetch_scanning_systems, update_scanning_schedule,
};
use crate::api::models::{ScanSchedulePolicyResponse, UpdateScanSchedulePolicyRequest};
use crate::routes::Route;

#[component]
pub fn ScanningView() -> Element {
    let nav = navigator();
    let mut tab = use_signal(|| "queue".to_string());
    let mut show_activity = use_signal(|| true);
    let mut schedule_open = use_signal(|| false);

    let stats = use_resource(|| async { fetch_scanning_stats().await });
    let queue = use_resource(|| async { fetch_scanning_queue(Some(50)).await });
    let systems = use_resource(|| async { fetch_scanning_systems(Some(100)).await });
    let activity = use_resource(|| async { fetch_scanning_activity(Some(20)).await });
    let mut schedule = use_resource(|| async { fetch_scanning_schedule().await });

    let schedule_value: Option<ScanSchedulePolicyResponse> =
        schedule.read().as_ref().and_then(|r| r.clone().ok());

    rsx! {
        div { style: "display:flex; flex-direction:column; gap:16px;",
            div { class: "page-head",
                div {
                    h1 { class: "page-title", "Scanning" }
                    p { class: "page-subtitle", "CVE scanning · vulnix live data" }
                }
                div { style: "display:flex; gap:8px;",
                    button { class: "btn btn-ghost focus-ring", onclick: move |_| schedule_open.set(true), "Schedule" }
                    button { class: "btn btn-primary focus-ring", "Rescan all" }
                }
            }

            div { class: "stat-strip",
                if let Some(Ok(s)) = stats.read().as_ref() {
                    { stat_card("Scanning now", &s.scanning.to_string(), Some(&format!("{} queued", s.queued)), "#60a5fa") }
                    { stat_card("Stale", &s.stale.to_string(), Some("past rescan interval"), "#fbbf24") }
                    { stat_card("Never scanned", &s.never_scanned.to_string(), None, "#9ca3af") }
                    { stat_card("Failed", &s.failed.to_string(), None, "#f87171") }
                    { stat_card("Coverage", &format!("{}%", s.coverage_percent), Some("configs with results"), "#34d399") }
                } else {
                    { stat_card("Scanning now", "—", None, "#60a5fa") }
                    { stat_card("Stale", "—", None, "#fbbf24") }
                    { stat_card("Never scanned", "—", None, "#9ca3af") }
                    { stat_card("Failed", "—", None, "#f87171") }
                    { stat_card("Coverage", "—", None, "#34d399") }
                }
            }

            div {
                style: if show_activity() { "display:grid; grid-template-columns: 1fr 320px; gap:14px; align-items:start;" } else { "display:grid; grid-template-columns: 1fr; gap:14px; align-items:start;" },
                div { class: "card", style: "overflow:hidden;",
                    div { class: "sd-tabs", style: "padding:0 16px; border-bottom:1px solid var(--cf-card-border); display:flex; align-items:center;",
                        button { class: if tab() == "queue" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" }, onclick: move |_| tab.set("queue".to_string()), "Active & Recent" }
                        button { class: if tab() == "all" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" }, onclick: move |_| tab.set("all".to_string()), "All configs" }
                    }

                    if tab() == "queue" {
                        table { class: "sys-table",
                            thead { tr { th { "Config" } th { "Status" } th { "Findings" } th { "Last scan" } th { style: "text-align:right;", " " } } }
                            tbody {
                                if let Some(Ok(rows)) = queue.read().as_ref() {
                                    for row in rows.iter() {
                                        tr {
                                            td {
                                                div { style: "font-weight:600; font-size:13px;", "{row.hostname}" }
                                                div { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "{row.flake_name.clone().unwrap_or_default()}" }
                                                div { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "{row.commit_hash.clone().unwrap_or_default()}" }
                                            }
                                            td { span { class: "chip chip-info", "{row.status}" } }
                                            td {
                                                div { style: "display:flex; gap:4px;",
                                                    if row.critical_count > 0 { span { class: "chip chip-critical", style: "font-size:10px;", "{row.critical_count}C" } }
                                                    if row.high_count > 0 { span { class: "chip chip-warning", style: "font-size:10px;", "{row.high_count}H" } }
                                                    if row.medium_count > 0 { span { class: "chip chip-info", style: "font-size:10px;", "{row.medium_count}M" } }
                                                }
                                            }
                                            td { style: "font-size:12px; color:var(--cf-text-muted);", "{row.completed_at.map(|d| d.to_rfc3339()).unwrap_or_default()}" }
                                            td { div { class: "row-actions", button { class: "btn-icon focus-ring", onclick: move |_| { let _ = nav.push(Route::CvesView {}); }, "→" } } }
                                        }
                                    }
                                } else {
                                    tr { td { colspan: 5, style: "padding:14px; color:var(--cf-text-muted);", "Loading queue…" } }
                                }
                            }
                        }
                    } else {
                        table { class: "sys-table",
                            thead { tr { th { "System" } th { "Env" } th { "Configs" } th { "Fresh" } th { "Current findings" } } }
                            tbody {
                                if let Some(Ok(rows)) = systems.read().as_ref() {
                                    for s in rows.iter() {
                                        tr {
                                            td { div { style: "font-weight:600; font-size:13px;", "{s.hostname}" } }
                                            td { span { class: "chip chip-info", style: "font-size:10px;", "{s.environment.clone().unwrap_or_default()}" } }
                                            td { class: "mono", style: "font-size:12px;", "{s.total_configs}" }
                                            td { class: "mono", style: "font-size:12px;", "{s.scanned}/{s.total_configs}" }
                                            td {
                                                div { style: "display:flex; gap:4px;",
                                                    if s.current_crit > 0 { span { class: "chip chip-critical", style: "font-size:10px;", "{s.current_crit}C" } }
                                                    if s.current_high > 0 { span { class: "chip chip-warning", style: "font-size:10px;", "{s.current_high}H" } }
                                                    if s.current_crit == 0 && s.current_high == 0 { span { class: "chip chip-healthy", style: "font-size:10px;", "clean" } }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    tr { td { colspan: 5, style: "padding:14px; color:var(--cf-text-muted);", "Loading systems…" } }
                                }
                            }
                        }
                    }
                }

                if show_activity() {
                    div { class: "card", style: "padding:16px;",
                        div { style: "display:flex; align-items:center; justify-content:space-between; margin-bottom:12px;",
                            h3 { style: "margin:0; font-size:13px; font-weight:600;", "Scan activity" }
                            button { class: "btn-icon focus-ring", onclick: move |_| show_activity.set(false), "×" }
                        }
                        div { class: "dash-w-body", style: "gap:0;",
                            if let Some(Ok(items)) = activity.read().as_ref() {
                                for item in items.iter() {
                                    div { style: "display:flex; gap:10px; padding-left:2px; padding-bottom:10px;",
                                        div { style: "width:22px; height:22px; border-radius:6px; background:color-mix(in oklab, #60a5fa 18%, transparent); color:#60a5fa; display:grid; place-items:center; font-size:11px;", "↻" }
                                        div {
                                            div { style: "font-size:12px; display:flex; justify-content:space-between; gap:6px;", span { style: "font-weight:600;", "{item.event}" } span { style: "font-size:11px; color:var(--cf-text-muted);", "{item.at.map(|d| d.to_rfc3339()).unwrap_or_default()}" } }
                                            div { class: "mono", style: "font-size:11px; color:var(--cf-brand-purple);", "{item.name}" }
                                            div { style: "font-size:11px; color:var(--cf-text-muted);", "{item.detail}" }
                                        }
                                    }
                                }
                            } else {
                                div { style: "font-size:12px; color:var(--cf-text-muted);", "Loading activity…" }
                            }
                        }
                    }
                }
            }

            if schedule_open() {
                div { class: "modal-backdrop", onclick: move |_| schedule_open.set(false),
                    div { class: "modal", style: "width:min(620px,96vw);", onclick: move |evt| evt.stop_propagation(),
                        div { class: "modal-head", h2 { "Scan schedule" }, p { "Control how often vulnix rescans configurations." } }
                        div { class: "modal-body",
                            if let Some(policy) = schedule_value.clone() {
                                div { style: "display:flex; flex-direction:column; gap:10px;",
                                    div { class: "page-subtitle", "Current policy loaded from backend:" }
                                    div { class: "mono", style: "font-size:12px;", "on_build={policy.on_build}, deployed={policy.deployed_interval}, recent={policy.recent_interval}, archived={policy.archived_interval}, archived_enabled={policy.archived_enabled}, rebuild_to_scan={policy.rebuild_to_scan}" }
                                }
                            } else {
                                div { class: "page-subtitle", "Loading schedule…" }
                            }
                        }
                        div { class: "modal-foot",
                            button { class: "btn btn-ghost focus-ring", onclick: move |_| schedule_open.set(false), "Cancel" }
                            button {
                                class: "btn btn-primary focus-ring",
                                onclick: move |_| {
                                    spawn(async move {
                                        if let Ok(current) = fetch_scanning_schedule().await {
                                            let req = UpdateScanSchedulePolicyRequest {
                                                on_build: current.on_build,
                                                deployed_interval: current.deployed_interval,
                                                recent_interval: current.recent_interval,
                                                archived_interval: current.archived_interval,
                                                archived_enabled: current.archived_enabled,
                                                rebuild_to_scan: current.rebuild_to_scan,
                                            };
                                            let _ = update_scanning_schedule(&req).await;
                                        }
                                    });
                                    schedule.restart();
                                    schedule_open.set(false);
                                },
                                "Save schedule"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn stat_card(label: &str, value: &str, meta: Option<&str>, color: &str) -> Element {
    rsx! {
        div {
            class: "stat",
            span { class: "stat-accent", style: "--stat-color:{color};" }
            div { class: "stat-label", "{label}" }
            div { class: "stat-value", style: "color:{color};", "{value}" }
            if let Some(m) = meta {
                div { class: "stat-meta", "{m}" }
            }
        }
    }
}
