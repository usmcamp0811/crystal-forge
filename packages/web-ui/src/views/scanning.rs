use dioxus::prelude::*;
use std::collections::HashSet;

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
    let mut expanded_systems = use_signal(HashSet::<String>::new);

    let mut policy_on_build = use_signal(|| true);
    let mut policy_deployed_interval = use_signal(|| "24h".to_string());
    let mut policy_recent_interval = use_signal(|| "24h".to_string());
    let mut policy_archived_interval = use_signal(|| "168h".to_string());
    let mut policy_archived_enabled = use_signal(|| true);
    let mut policy_rebuild_to_scan = use_signal(|| false);

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
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| {
                            if let Some(policy) = schedule.read().as_ref().and_then(|r| r.clone().ok()) {
                                policy_on_build.set(policy.on_build);
                                policy_deployed_interval.set(policy.deployed_interval);
                                policy_recent_interval.set(policy.recent_interval);
                                policy_archived_interval.set(policy.archived_interval);
                                policy_archived_enabled.set(policy.archived_enabled);
                                policy_rebuild_to_scan.set(policy.rebuild_to_scan);
                            }
                            schedule_open.set(true);
                        },
                        "Schedule"
                    }
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
                            thead { tr { th { "System" } th { "Env" } th { "Configs" } th { "Fresh" } th { "Current findings" } th { " " } } }
                            tbody {
                                if let Some(Ok(rows)) = systems.read().as_ref() {
                                    for s in rows.iter() {
                                        {
                                            let hostname = s.hostname.clone();
                                            let is_expanded = expanded_systems.read().contains(&hostname);
                                            let queue_rows_for_host = queue
                                                .read()
                                                .as_ref()
                                                .and_then(|res| res.as_ref().ok())
                                                .map(|all| {
                                                    all.iter()
                                                        .filter(|q| q.hostname == hostname)
                                                        .cloned()
                                                        .collect::<Vec<_>>()
                                                })
                                                .unwrap_or_default();

                                            rsx! {
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
                                                    td {
                                                        style: "text-align:right;",
                                                        button {
                                                            class: "btn-icon focus-ring",
                                                            onclick: {
                                                                let host = s.hostname.clone();
                                                                move |_| {
                                                                    let mut next = expanded_systems.read().clone();
                                                                    if next.contains(&host) {
                                                                        next.remove(&host);
                                                                    } else {
                                                                        next.insert(host.clone());
                                                                    }
                                                                    expanded_systems.set(next);
                                                                }
                                                            },
                                                            if is_expanded { "▾" } else { "▸" }
                                                        }
                                                    }
                                                }

                                                if is_expanded {
                                                    tr {
                                                        td { colspan: 6, style: "padding:0;",
                                                            div { style: "padding:8px 12px 12px 20px; border-top:1px solid var(--cf-card-border); background:color-mix(in oklab, var(--cf-card-bg) 92%, #60a5fa 8%);",
                                                                table { class: "sys-table", style: "margin:0; border:none;",
                                                                    thead {
                                                                        tr {
                                                                            th { "Commit" }
                                                                            th { "Status" }
                                                                            th { "Findings" }
                                                                            th { "Last scan" }
                                                                            th { " " }
                                                                        }
                                                                    }
                                                                    tbody {
                                                                        if queue_rows_for_host.is_empty() {
                                                                            tr {
                                                                                td { colspan: 5, style: "padding:10px; color:var(--cf-text-muted);", "No per-config scan rows yet." }
                                                                            }
                                                                        } else {
                                                                            for row in queue_rows_for_host.iter() {
                                                                                tr {
                                                                                    td {
                                                                                        div { class: "mono", style: "font-size:12px;", "{commit_label(&row.commit_hash)}" }
                                                                                        div { style: "font-size:11px; color:var(--cf-text-muted);", "{row.flake_name.clone().unwrap_or_default()}" }
                                                                                    }
                                                                                    td { span { class: "chip chip-info", "{row.status}" } }
                                                                                    td {
                                                                                        div { style: "display:flex; gap:4px;",
                                                                                            if row.critical_count > 0 { span { class: "chip chip-critical", style: "font-size:10px;", "{row.critical_count}C" } }
                                                                                            if row.high_count > 0 { span { class: "chip chip-warning", style: "font-size:10px;", "{row.high_count}H" } }
                                                                                            if row.medium_count > 0 { span { class: "chip chip-info", style: "font-size:10px;", "{row.medium_count}M" } }
                                                                                            if row.critical_count == 0 && row.high_count == 0 && row.medium_count == 0 { span { class: "chip chip-healthy", style: "font-size:10px;", "clean" } }
                                                                                        }
                                                                                    }
                                                                                    td { style: "font-size:12px; color:var(--cf-text-muted);", "{row.completed_at.map(|d| d.to_rfc3339()).unwrap_or_default()}" }
                                                                                    td { div { class: "row-actions", button { class: "btn-icon focus-ring", onclick: move |_| { let _ = nav.push(Route::CvesView {}); }, "→" } } }
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
                                } else {
                                    tr { td { colspan: 6, style: "padding:14px; color:var(--cf-text-muted);", "Loading systems…" } }
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
                                    div { style: "display:flex; justify-content:space-between; align-items:center;",
                                        span { style: "font-size:13px; font-weight:600;", "Scan on build" }
                                        input {
                                            r#type: "checkbox",
                                            checked: policy_on_build(),
                                            onchange: move |e| policy_on_build.set(e.checked())
                                        }
                                    }

                                    div { style: "display:grid; grid-template-columns: 1fr 1fr; gap:10px;",
                                        div {
                                            label { style: "display:block; font-size:12px; color:var(--cf-text-muted); margin-bottom:4px;", "Deployed interval" }
                                            select {
                                                class: "input",
                                                value: policy_deployed_interval(),
                                                oninput: move |e| policy_deployed_interval.set(e.value()),
                                                {interval_option("24h")}
                                                {interval_option("48h")}
                                                {interval_option("72h")}
                                                {interval_option("168h")}
                                            }
                                        }
                                        div {
                                            label { style: "display:block; font-size:12px; color:var(--cf-text-muted); margin-bottom:4px;", "Recent interval" }
                                            select {
                                                class: "input",
                                                value: policy_recent_interval(),
                                                oninput: move |e| policy_recent_interval.set(e.value()),
                                                {interval_option("24h")}
                                                {interval_option("48h")}
                                                {interval_option("72h")}
                                                {interval_option("168h")}
                                            }
                                        }
                                        div {
                                            label { style: "display:block; font-size:12px; color:var(--cf-text-muted); margin-bottom:4px;", "Archived interval" }
                                            select {
                                                class: "input",
                                                value: policy_archived_interval(),
                                                oninput: move |e| policy_archived_interval.set(e.value()),
                                                {interval_option("24h")}
                                                {interval_option("48h")}
                                                {interval_option("72h")}
                                                {interval_option("168h")}
                                                {interval_option("336h")}
                                            }
                                        }
                                    }

                                    div { style: "display:flex; justify-content:space-between; align-items:center;",
                                        span { style: "font-size:13px; font-weight:600;", "Include archived configs" }
                                        input {
                                            r#type: "checkbox",
                                            checked: policy_archived_enabled(),
                                            onchange: move |e| policy_archived_enabled.set(e.checked())
                                        }
                                    }

                                    div { style: "display:flex; justify-content:space-between; align-items:center;",
                                        span { style: "font-size:13px; font-weight:600;", "Rebuild missing derivations before scan" }
                                        input {
                                            r#type: "checkbox",
                                            checked: policy_rebuild_to_scan(),
                                            onchange: move |e| policy_rebuild_to_scan.set(e.checked())
                                        }
                                    }

                                    div { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "Last updated: {policy.updated_at.to_rfc3339()}" }
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
                                    let req = UpdateScanSchedulePolicyRequest {
                                        on_build: policy_on_build(),
                                        deployed_interval: policy_deployed_interval(),
                                        recent_interval: policy_recent_interval(),
                                        archived_interval: policy_archived_interval(),
                                        archived_enabled: policy_archived_enabled(),
                                        rebuild_to_scan: policy_rebuild_to_scan(),
                                    };
                                    spawn(async move {
                                        let _ = update_scanning_schedule(&req).await;
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

fn interval_option(value: &'static str) -> Element {
    rsx! {
        option { value: value, "{value}" }
    }
}

fn commit_label(commit_hash: &Option<String>) -> String {
    match commit_hash {
        Some(hash) if !hash.is_empty() => hash.clone(),
        _ => "unknown".to_string(),
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
