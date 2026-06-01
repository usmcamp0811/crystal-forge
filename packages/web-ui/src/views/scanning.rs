use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::api::client::{
    fetch_scanning_activity, fetch_scanning_queue, fetch_scanning_schedule, fetch_scanning_stats,
    fetch_scanning_system_scans, fetch_scanning_systems, update_scanning_schedule,
};
use crate::api::models::ScanningQueueItemResponse;
use crate::api::models::{ScanSchedulePolicyResponse, UpdateScanSchedulePolicyRequest};
use crate::components::chips::EnvBadge;
use crate::components::icon::{Icon, IconName};
use crate::routes::Route;

/// Status presentation metadata mirroring the design's SCAN_STATUS_META map.
struct StatusMeta {
    cls: &'static str,
    color: &'static str,
    label: &'static str,
}

fn status_meta(status: &str) -> StatusMeta {
    match status {
        "in_progress" | "scanning" => StatusMeta {
            cls: "chip-info",
            color: "#60a5fa",
            label: "scanning",
        },
        "pending" | "queued" => StatusMeta {
            cls: "chip-unknown",
            color: "#9ca3af",
            label: "queued",
        },
        "failed" => StatusMeta {
            cls: "chip-critical",
            color: "#f87171",
            label: "failed",
        },
        "needs-build" | "needs_build" => StatusMeta {
            cls: "chip-unknown",
            color: "#f59e0b",
            label: "needs build",
        },
        "stale" => StatusMeta {
            cls: "chip-warning",
            color: "#fbbf24",
            label: "stale",
        },
        "unscanned" => StatusMeta {
            cls: "chip-unknown",
            color: "#6b7280",
            label: "never scanned",
        },
        _ => StatusMeta {
            cls: "chip-healthy",
            color: "#34d399",
            label: "clean",
        },
    }
}

/// Findings-aware status label: a completed scan with findings reads as "CVEs found".
fn effective_status(row: &ScanningQueueItemResponse) -> String {
    let has_findings = row.critical_count > 0 || row.high_count > 0;
    if row.status == "completed" && has_findings {
        return "has-cves".to_string();
    }
    row.status.clone()
}

fn has_cves_meta() -> StatusMeta {
    StatusMeta {
        cls: "chip-critical",
        color: "#f87171",
        label: "CVEs found",
    }
}

fn meta_for(status: &str) -> StatusMeta {
    if status == "has-cves" {
        has_cves_meta()
    } else {
        status_meta(status)
    }
}

#[component]
pub fn ScanningView() -> Element {
    let nav = navigator();
    let mut tab = use_signal(|| "queue".to_string());
    let mut all_configs_search = use_signal(String::new);
    let mut all_configs_env_filter = use_signal(|| "all".to_string());
    let mut show_activity = use_signal(|| true);
    let mut schedule_open = use_signal(|| false);
    let mut expanded_systems = use_signal(HashSet::<String>::new);
    let mut system_scan_rows = use_signal(HashMap::<String, Vec<ScanningQueueItemResponse>>::new);
    let mut loading_system_scans = use_signal(HashSet::<String>::new);

    let mut policy_on_build = use_signal(|| true);
    let mut policy_deployed_interval = use_signal(|| "24h".to_string());
    let mut policy_recent_interval = use_signal(|| "24h".to_string());
    let mut policy_archived_interval = use_signal(|| "168h".to_string());
    let mut policy_archived_enabled = use_signal(|| true);
    let mut policy_rebuild_to_scan = use_signal(|| false);
    let mut schedule_save_error = use_signal(|| Option::<String>::None);

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
                    p { class: "page-subtitle", "CVE scanning · vulnix · live data" }
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
                            schedule_save_error.set(None);
                            schedule_open.set(true);
                        },
                        Icon { name: IconName::Gear, size: 14 }
                        " Schedule"
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        disabled: true,
                        title: "Fleet rescan endpoint is not available yet",
                        Icon { name: IconName::Sync, size: 14 }
                        " Rescan all"
                    }
                }
            }

            div { class: "stat-strip",
                if let Some(Ok(s)) = stats.read().as_ref() {
                    { stat_card("Scanning now", &s.scanning.to_string(), Some(&format!("{} queued", s.queued)), "#60a5fa") }
                    { stat_card("Stale", &s.stale.to_string(), Some("past rescan interval"), "#fbbf24") }
                    { stat_card("Never scanned", &s.never_scanned.to_string(), None, "#9ca3af") }
                    { stat_card("Failed", &s.failed.to_string(), None, if s.failed > 0 { "#f87171" } else { "#34d399" }) }
                    { stat_card("Coverage", &format!("{}%", s.coverage_percent), Some("configs with results"), "#34d399") }
                } else {
                    { stat_card("Scanning now", "—", None, "#60a5fa") }
                    { stat_card("Stale", "—", None, "#fbbf24") }
                    { stat_card("Never scanned", "—", None, "#9ca3af") }
                    { stat_card("Failed", "—", None, "#34d399") }
                    { stat_card("Coverage", "—", None, "#34d399") }
                }
            }

            div {
                style: if show_activity() { "display:grid; grid-template-columns: 1fr 320px; gap:14px; align-items:start;" } else { "display:grid; grid-template-columns: 1fr; gap:14px; align-items:start;" },
                div { class: "card", style: "overflow:hidden;",
                    div { class: "sd-tabs", style: "padding:0 16px; border-bottom:1px solid var(--cf-card-border); display:flex; align-items:center;",
                        button { class: if tab() == "queue" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" }, onclick: move |_| tab.set("queue".to_string()), "Active & Recent" }
                        button { class: if tab() == "all" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" }, onclick: move |_| tab.set("all".to_string()), "All configs" }
                        if !show_activity() {
                            button {
                                class: "btn btn-ghost focus-ring",
                                style: "margin-left:auto; font-size:11px; padding:2px 8px;",
                                title: "Show scan activity",
                                onclick: move |_| show_activity.set(true),
                                Icon { name: IconName::Rows, size: 11 }
                                " Activity"
                            }
                        }
                    }

                    if tab() == "queue" {
                        table { class: "sys-table",
                            thead { tr {
                                th { "Config" }
                                th { "Freshness" }
                                th { "Status" }
                                th { "Findings" }
                                th { "Last scan" }
                                th { "Trigger" }
                                th { style: "text-align:right;", " " }
                            } }
                            tbody {
                                if let Some(Ok(rows)) = queue.read().as_ref() {
                                    for row in rows.iter() {
                                        {
                                            let eff = effective_status(row);
                                            let meta = meta_for(&eff);
                                            let fresh = freshness_label(row);
                                            let trigger = row.trigger.clone();
                                            let has_findings = row.critical_count > 0 || row.high_count > 0;
                                            rsx! {
                                                tr {
                                                    td {
                                                        div { style: "font-weight:600; font-size:13px;", "{row.hostname}" }
                                                        div { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "{flake_commit(row)}" }
                                                    }
                                                    td { { fresh_chip(&fresh) } }
                                                    td {
                                                        span { class: "chip {meta.cls}",
                                                            span { class: "chip-dot", style: "background:{meta.color};" }
                                                            "{meta.label}"
                                                        }
                                                    }
                                                    td { { findings_cell(row.critical_count, row.high_count, row.medium_count, row.completed_at.is_some()) } }
                                                    td { style: "font-size:12px; color:var(--cf-text-muted);", "{last_scan(row)}" }
                                                    td {
                                                        match trigger {
                                                            Some(t) if !t.is_empty() => rsx! { span { class: "chip chip-unknown", style: "font-size:10px;", "{t}" } },
                                                            _ => rsx! { span { style: "font-size:11px; color:var(--cf-text-muted);", "—" } },
                                                        }
                                                    }
                                                    td {
                                                        div { class: "row-actions",
                                                            button { class: "btn-icon focus-ring", disabled: true, title: "Rescan endpoint not available yet", Icon { name: IconName::Sync, size: 14 } }
                                                            if has_findings {
                                                                button { class: "btn-icon focus-ring", title: "View CVEs", onclick: move |_| { let _ = nav.push(Route::CvesView {}); }, Icon { name: IconName::ArrowRight, size: 14 } }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    tr { td { colspan: 7, style: "padding:14px; color:var(--cf-text-muted);", "Loading queue…" } }
                                }
                            }
                        }
                    } else {
                        if let Some(Ok(rows)) = systems.read().as_ref() {
                            {
                                let total_configs: i64 = rows.iter().map(|r| r.total_configs).sum();
                                let visible = rows
                                    .iter()
                                    .filter(|r| {
                                        let search = all_configs_search().to_lowercase();
                                        let env_filter = all_configs_env_filter();
                                        let env_name = r.environment.clone().unwrap_or_default();
                                        let ms = search.is_empty() || r.hostname.to_lowercase().contains(&search);
                                        let me = env_filter == "all" || env_name == env_filter;
                                        ms && me
                                    })
                                    .count();
                                let mut envs = rows
                                    .iter()
                                    .filter_map(|r| r.environment.clone())
                                    .filter(|e| !e.is_empty())
                                    .collect::<Vec<_>>();
                                envs.sort();
                                envs.dedup();
                                rsx! {
                                    div { style: "padding:10px 16px; border-bottom:1px solid var(--cf-divider); display:flex; gap:10px; align-items:center; flex-wrap:wrap;",
                                        div { class: "filter-search", style: "max-width:240px;",
                                            Icon { name: IconName::Search, size: 14 }
                                            input {
                                                class: "input focus-ring",
                                                placeholder: "Search systems…",
                                                value: all_configs_search(),
                                                oninput: move |e| all_configs_search.set(e.value()),
                                            }
                                        }
                                        select {
                                            class: "input filter-select focus-ring",
                                            style: "width:auto;",
                                            value: all_configs_env_filter(),
                                            oninput: move |e| all_configs_env_filter.set(e.value()),
                                            option { value: "all", "All environments" }
                                            for env in envs {
                                                option { value: "{env}", "{env}" }
                                            }
                                        }
                                        span { class: "filter-count", "{visible} systems · {total_configs} configs" }
                                    }
                                }
                            }
                        }

                        table { class: "sys-table",
                            thead { tr {
                                th { "System" }
                                th { "Env" }
                                th { "Configs" }
                                th { title: "Share of this system's configs that have a fresh scan (green), a stale scan past the rescan interval (amber), need a build (orange), or were never scanned (gray)", "Scan freshness" }
                                th { "Current findings" }
                                th { style: "text-align:right;", " " }
                            } }
                            tbody {
                                if let Some(Ok(rows)) = systems.read().as_ref() {
                                    for s in rows.iter() {
                                        {
                                            let search = all_configs_search().to_lowercase();
                                            let env_filter = all_configs_env_filter();
                                            let env_name = s.environment.clone().unwrap_or_default();
                                            let matches_search = search.is_empty() || s.hostname.to_lowercase().contains(&search);
                                            let matches_env = env_filter == "all" || env_name == env_filter;

                                            if matches_search && matches_env {
                                                let hostname = s.hostname.clone();
                                                let system_key = s.system_id.to_string();
                                                let is_expanded = expanded_systems.read().contains(&hostname);
                                                let queue_rows_for_system = system_scan_rows
                                                    .read()
                                                    .get(&system_key)
                                                    .cloned()
                                                    .unwrap_or_default();
                                                let system_scans_loading = loading_system_scans.read().contains(&system_key);

                                                let total = s.total_configs.max(1) as f64;
                                                let scanned_pct = (s.scanned as f64 / total) * 100.0;
                                                let stale_pct = (s.stale as f64 / total) * 100.0;
                                                let needs_pct = (s.needs_build as f64 / total) * 100.0;
                                                let unscanned_pct = (s.unscanned as f64 / total) * 100.0;

                                                rsx! {
                                                    tr {
                                                        style: "cursor:pointer;",
                                                        onclick: {
                                                            let host = hostname.clone();
                                                            let sid = s.system_id;
                                                            let key = system_key.clone();
                                                            move |_| toggle_system(
                                                                host.clone(), sid, key.clone(),
                                                                expanded_systems, system_scan_rows, loading_system_scans,
                                                            )
                                                        },
                                                        td {
                                                            div { style: "display:flex; align-items:center; gap:8px;",
                                                                span { style: "color:var(--cf-text-muted); flex-shrink:0; display:inline-flex;",
                                                                    Icon { name: if is_expanded { IconName::ChevronDown } else { IconName::ChevronRight }, size: 12 }
                                                                }
                                                                div {
                                                                    div { style: "font-weight:600; font-size:13px;", "{s.hostname}" }
                                                                }
                                                            }
                                                        }
                                                        td {
                                                            if let Some(env) = s.environment.clone() {
                                                                EnvBadge { name: env }
                                                            }
                                                        }
                                                        td { class: "mono", style: "font-size:12px;", "{s.total_configs}" }
                                                        td {
                                                            div { style: "display:flex; align-items:center; gap:8px; min-width:120px;",
                                                                title: "{s.scanned} fresh · {s.stale} stale · {s.needs_build} need build · {s.unscanned} never scanned",
                                                                div { style: "flex:1; height:5px; background:var(--cf-subtle-bg); border-radius:99px; overflow:hidden; display:flex;",
                                                                    div { style: "width:{scanned_pct}%; background:#34d399;" }
                                                                    div { style: "width:{stale_pct}%; background:#fbbf24;" }
                                                                    div { style: "width:{needs_pct}%; background:#f59e0b;" }
                                                                    div { style: "width:{unscanned_pct}%; background:#4b5563;" }
                                                                }
                                                                span { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "{s.scanned}/{s.total_configs}" }
                                                            }
                                                            div { style: "font-size:10px; color:var(--cf-text-muted); margin-top:3px; display:flex; gap:8px; flex-wrap:wrap;",
                                                                span { style: "color:#34d399;", "{s.scanned} fresh" }
                                                                if s.stale > 0 { span { style: "color:#fbbf24;", "{s.stale} stale" } }
                                                                if s.needs_build > 0 { span { style: "color:#f59e0b;", "{s.needs_build} need build" } }
                                                                if s.unscanned > 0 { span { "{s.unscanned} never" } }
                                                            }
                                                        }
                                                        td {
                                                            if s.current_crit > 0 || s.current_high > 0 {
                                                                div { style: "display:flex; gap:4px;",
                                                                    if s.current_crit > 0 { span { class: "chip chip-critical", style: "font-size:10px;", "{s.current_crit}C" } }
                                                                    if s.current_high > 0 { span { class: "chip chip-warning", style: "font-size:10px;", "{s.current_high}H" } }
                                                                }
                                                            } else {
                                                                 span { class: "chip chip-healthy", style: "font-size:10px; display:inline-flex; align-items:center; gap:4px;", Icon { name: IconName::Check, size: 9 } " clean" }
                                                            }
                                                        }
                                                        td {
                                                            style: "text-align:right;",
                                                            div { class: "row-actions", style: "justify-content:flex-end;",
                                                                button {
                                                                    class: "btn-icon focus-ring",
                                                                    disabled: true,
                                                                    title: "Rescan endpoint not available yet",
                                                                    onclick: move |evt| evt.stop_propagation(),
                                                                    Icon { name: IconName::Sync, size: 14 }
                                                                }
                                                            }
                                                        }
                                                    }

                                                    if is_expanded {
                                                        tr {
                                                            td { colspan: 6, style: "padding:0; background:color-mix(in oklab, var(--cf-brand-purple) 4%, var(--cf-page-bg));",
                                                                div { style: "padding:6px 16px 10px 40px;",
                                                                    div { style: "display:flex; justify-content:space-between; align-items:center; padding:4px 8px;",
                                                                        span { style: "font-size:11px; color:var(--cf-text-muted);",
                                                                            "{queue_rows_for_system.len()} config(s) for this system · newest first"
                                                                        }
                                                                        button { class: "btn btn-ghost focus-ring", style: "font-size:11px; padding:2px 8px;", disabled: true, title: "Rescan endpoint not available yet", Icon { name: IconName::Sync, size: 10 } " Rescan all" }
                                                                    }
                                                                    div { style: "border:1px solid var(--cf-divider); border-radius:8px; overflow:hidden;",
                                                                        table { style: "width:100%; border-collapse:collapse; font-size:12px;",
                                                                            thead {
                                                                                tr { style: "color:var(--cf-text-muted); font-size:10px; text-transform:uppercase; letter-spacing:0.06em; background:var(--cf-card-bg);",
                                                                                    th { style: "text-align:left; padding:6px 8px; font-weight:600;", "Commit" }
                                                                                    th { style: "text-align:left; padding:6px 8px; font-weight:600;", "Freshness" }
                                                                                    th { style: "text-align:left; padding:6px 8px; font-weight:600;", "Status" }
                                                                                    th { style: "text-align:left; padding:6px 8px; font-weight:600;", "Findings" }
                                                                                    th { style: "text-align:left; padding:6px 8px; font-weight:600;", "Last scan" }
                                                                                    th { style: "text-align:right; padding:6px 8px;", " " }
                                                                                }
                                                                            }
                                                                            tbody {
                                                                                if system_scans_loading {
                                                                                    tr { td { colspan: 6, style: "padding:10px; color:var(--cf-text-muted);", "Loading per-config scan rows…" } }
                                                                                } else if queue_rows_for_system.is_empty() {
                                                                                    tr { td { colspan: 6, style: "padding:10px; color:var(--cf-text-muted);", "No per-config scan rows yet." } }
                                                                                } else {
                                                                    for row in queue_rows_for_system.iter() {
                                                                        {
                                                                            let eff = effective_status(row);
                                                                            let meta = meta_for(&eff);
                                                                            let fresh = freshness_label(row);
                                                                            let is_current = row.is_current;
                                                                            let needs_build = row.status == "needs-build" || row.status == "needs_build";
                                                                                            rsx! {
                                                                                                tr { style: "border-top:1px solid var(--cf-divider);",
                                                                                                    td { style: "padding:7px 8px;",
                                                                                                        span { class: "mono", style: "font-weight:600;", "{commit_label(&row.commit_hash)}" }
                                                                                                        if is_current { span { class: "chip chip-info", style: "font-size:9px; margin-left:6px;", "current" } }
                                                                                                        div { style: "font-size:10px; color:var(--cf-text-muted);", "{row.flake_name.clone().unwrap_or_default()}" }
                                                                                                    }
                                                                                                    td { style: "padding:7px 8px;", { fresh_chip(&fresh) } }
                                                                                                    td { style: "padding:7px 8px;",
                                                                                                        span { class: "chip {meta.cls}", style: "font-size:10px;",
                                                                                                            span { class: "chip-dot", style: "background:{meta.color};" }
                                                                                                            "{meta.label}"
                                                                                                        }
                                                                                                    }
                                                                                                    td { style: "padding:7px 8px;", { findings_cell(row.critical_count, row.high_count, row.medium_count, row.completed_at.is_some()) } }
                                                                                                    td { style: "padding:7px 8px; color:var(--cf-text-muted);", "{last_scan(row)}" }
                                                                                    td { style: "padding:7px 8px; text-align:right;",
                                                                                        if needs_build {
                                                                                             button { class: "btn btn-ghost focus-ring", style: "font-size:11px; padding:2px 8px;", disabled: true, title: "Not in cache — build first, then scan", Icon { name: IconName::Cpu, size: 11 } " Build & scan" }
                                                                                        } else {
                                                                                             button { class: "btn-icon focus-ring", disabled: true, title: "Rescan endpoint not available yet", Icon { name: IconName::Sync, size: 13 } }
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
                                                        }
                                                    }
                                                }
                                            } else {
                                                rsx! {}
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
                            button { class: "btn-icon focus-ring", title: "Hide panel", onclick: move |_| show_activity.set(false), Icon { name: IconName::X, size: 14 } }
                        }
                        div { class: "dash-w-body", style: "gap:0;",
                            if let Some(Ok(items)) = activity.read().as_ref() {
                                for (idx, item) in items.iter().enumerate() {
                                    div { style: "display:flex; gap:10px; padding-left:2px;",
                                        div { style: "display:flex; flex-direction:column; align-items:center; padding-top:4px; flex-shrink:0;",
                                            div { style: "width:22px; height:22px; border-radius:6px; background:color-mix(in oklab, #60a5fa 18%, transparent); color:#60a5fa; display:grid; place-items:center;", Icon { name: IconName::Sync, size: 11 } }
                                            if idx + 1 < items.len() {
                                                div { style: "width:2px; flex:1; background:var(--cf-divider); min-height:16px;" }
                                            }
                                        }
                                        div { style: if idx + 1 == items.len() { "padding-top:3px; padding-bottom:0; min-width:0;" } else { "padding-top:3px; padding-bottom:14px; min-width:0;" },
                                            div { style: "font-size:12px; display:flex; justify-content:space-between; gap:6px; min-width:0;",
                                                span { style: "font-weight:600;", "{item.event}" }
                                                span {
                                                    style: "font-size:11px; color:var(--cf-text-muted); white-space:nowrap; min-width:0; max-width:150px; overflow:hidden; text-overflow:ellipsis; flex-shrink:1;",
                                                    title: "{item.at.map(|d| d.to_rfc3339()).unwrap_or_default()}",
                                                    "{item.at.map(|d| d.to_rfc3339()).unwrap_or_default()}"
                                                }
                                            }
                                            div { class: "mono", style: "font-size:11px; color:var(--cf-brand-purple);", "{item.name}" }
                                            div { style: "font-size:11px; color:var(--cf-text-muted); margin-top:2px;", "{item.detail}" }
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
                        div { class: "modal-head",
                            h2 { Icon { name: IconName::Gear, size: 14 } " Scan schedule" }
                            p { "Control how often vulnix rescans configurations. New & deployed configs scan most often; old ones least." }
                        }
                        div { class: "modal-body",
                            if schedule_value.is_some() {
                                div { style: "display:flex; flex-direction:column;",
                                    if let Some(err) = schedule_save_error() {
                                        div { class: "chip chip-critical", style: "margin-bottom:8px;", "{err}" }
                                    }

                                    { schedule_row(
                                        "Scan on build",
                                        "Scan a freshly-built config before it can be deployed. Strongly recommended — the derivation is already in the store, so no extra build is needed.",
                                        rsx! {
                                            label { style: "display:flex; gap:8px; align-items:center; font-size:13px; cursor:pointer;",
                                                input {
                                                    r#type: "checkbox",
                                                    checked: policy_on_build(),
                                                    style: "accent-color:var(--cf-brand-purple);",
                                                    onchange: move |e| policy_on_build.set(e.checked())
                                                }
                                                span { if policy_on_build() { "On" } else { "Off" } }
                                            }
                                        },
                                    ) }

                                    { schedule_row(
                                        "Deployed configs",
                                        "Currently running on at least one system. Rescanned to catch newly-published advisories.",
                                        rsx! { { interval_select(policy_deployed_interval, false) } },
                                    ) }

                                    { schedule_row(
                                        "Recent configs",
                                        "Built in the last 30 days but not currently deployed.",
                                        rsx! { { interval_select(policy_recent_interval, false) } },
                                    ) }

                                    { schedule_row(
                                        "Archived configs",
                                        "Old / superseded configs no longer in rotation. Scan rarely (or never) to save builder time.",
                                        rsx! {
                                            div { style: "display:flex; align-items:center; gap:8px;",
                                                input {
                                                    r#type: "checkbox",
                                                    checked: policy_archived_enabled(),
                                                    style: "accent-color:var(--cf-brand-purple);",
                                                    onchange: move |e| policy_archived_enabled.set(e.checked())
                                                }
                                                { interval_select(policy_archived_interval, !policy_archived_enabled()) }
                                            }
                                        },
                                    ) }

                                    { schedule_row(
                                        "Rebuild to scan old configs",
                                        "vulnix needs a realised derivation. Archived configs evicted from cache must be rebuilt before they can be scanned — this can be expensive. Off = skip uncached configs instead of building them.",
                                        rsx! {
                                            label { style: "display:flex; gap:8px; align-items:center; font-size:13px; cursor:pointer;",
                                                input {
                                                    r#type: "checkbox",
                                                    checked: policy_rebuild_to_scan(),
                                                    style: "accent-color:var(--cf-brand-purple);",
                                                    onchange: move |e| policy_rebuild_to_scan.set(e.checked())
                                                }
                                                span { if policy_rebuild_to_scan() { "On" } else { "Off" } }
                                            }
                                        },
                                    ) }

                                    div { class: "sd-callout sd-callout-info", style: "font-size:11px; margin-top:12px;",
                                        Icon { name: IconName::Shield, size: 12 }
                                        div {
                                            "Estimated load: ~"
                                            if policy_on_build() { "every build" } else { "no" }
                                            " build scans + periodic rescans. Deployed configs at "
                                            strong { "{policy_deployed_interval()}" }
                                            " dominate builder cost."
                                        }
                                    }
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
                                    schedule_save_error.set(None);
                                    spawn(async move {
                                        match update_scanning_schedule(&req).await {
                                            Ok(_) => {
                                                schedule.restart();
                                                schedule_open.set(false);
                                            }
                                            Err(e) => {
                                                schedule_save_error.set(Some(format!("Failed to save schedule: {e}")));
                                            }
                                        }
                                    });
                                },
                                Icon { name: IconName::Check, size: 13 }
                                " Save schedule"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn toggle_system(
    host: String,
    system_id: uuid::Uuid,
    key: String,
    mut expanded_systems: Signal<HashSet<String>>,
    mut system_scan_rows: Signal<HashMap<String, Vec<ScanningQueueItemResponse>>>,
    mut loading_system_scans: Signal<HashSet<String>>,
) {
    let mut next = expanded_systems.read().clone();
    if next.contains(&host) {
        next.remove(&host);
    } else {
        next.insert(host.clone());

        if !system_scan_rows.read().contains_key(&key)
            && !loading_system_scans.read().contains(&key)
        {
            let fetch_key = key.clone();
            let mut loading = loading_system_scans.read().clone();
            loading.insert(key.clone());
            loading_system_scans.set(loading);

            spawn(async move {
                match fetch_scanning_system_scans(&system_id, Some(100)).await {
                    Ok(rows) => {
                        let mut next_rows = system_scan_rows.read().clone();
                        next_rows.insert(fetch_key.clone(), rows);
                        system_scan_rows.set(next_rows);
                    }
                    Err(_) => {
                        let mut next_rows = system_scan_rows.read().clone();
                        next_rows.insert(fetch_key.clone(), Vec::new());
                        system_scan_rows.set(next_rows);
                    }
                }

                let mut loading_next = loading_system_scans.read().clone();
                loading_next.remove(&fetch_key);
                loading_system_scans.set(loading_next);
            });
        }
    }
    expanded_systems.set(next);
}

/// Resolve a freshness class label, preferring the server-provided value and
/// falling back to client-side inference from scan recency.
fn freshness_label(row: &ScanningQueueItemResponse) -> String {
    if !row.freshness.is_empty() {
        return row.freshness.clone();
    }
    match row.completed_at {
        Some(ts) => {
            let age = chrono::Utc::now().signed_duration_since(ts);
            if age.num_hours() <= 24 {
                "deployed".to_string()
            } else if age.num_days() <= 30 {
                "recent".to_string()
            } else {
                "archived".to_string()
            }
        }
        None => "archived".to_string(),
    }
}

fn fresh_chip(freshness: &str) -> Element {
    let (cls, label) = match freshness {
        "deployed" => ("chip-healthy", "deployed"),
        "recent" => ("chip-info", "recent"),
        "archived" => ("chip-unknown", "archived"),
        other => ("chip-unknown", other),
    };
    rsx! {
        span { class: "chip {cls}", style: "font-size:10px;", "{label}" }
    }
}

fn findings_cell(crit: i32, high: i32, med: i32, scanned: bool) -> Element {
    if !scanned {
        return rsx! { span { style: "font-size:11px; color:var(--cf-text-muted);", "—" } };
    }
    rsx! {
        div { style: "display:flex; gap:4px;",
            if crit > 0 { span { class: "chip chip-critical", style: "font-size:10px;", "{crit}C" } }
            if high > 0 { span { class: "chip chip-warning", style: "font-size:10px;", "{high}H" } }
            if med > 0 { span { class: "chip chip-info", style: "font-size:10px;", "{med}M" } }
            if crit == 0 && high == 0 && med == 0 { span { class: "chip chip-healthy", style: "font-size:10px; display:inline-flex; align-items:center; gap:4px;", Icon { name: IconName::Check, size: 9 } " clean" } }
        }
    }
}

fn flake_commit(row: &ScanningQueueItemResponse) -> String {
    let flake = row.flake_name.clone().unwrap_or_default();
    let commit = commit_label(&row.commit_hash);
    if flake.is_empty() {
        commit
    } else {
        format!("{flake} · {commit}")
    }
}

fn last_scan(row: &ScanningQueueItemResponse) -> String {
    row.completed_at
        .map(|d| d.to_rfc3339())
        .unwrap_or_else(|| "—".to_string())
}

fn interval_select(mut value: Signal<String>, disabled: bool) -> Element {
    let options = ["1h", "6h", "12h", "24h", "7d", "30d", "168h", "336h", "never"];
    rsx! {
        select {
            class: "input focus-ring",
            style: "width:120px;",
            disabled,
            value: value(),
            oninput: move |e| value.set(e.value()),
            for opt in options {
                option {
                    value: "{opt}",
                    if opt == "never" { "Never" } else { "Every {opt}" }
                }
            }
        }
    }
}

fn schedule_row(title: &str, desc: &str, control: Element) -> Element {
    rsx! {
        div { style: "display:flex; align-items:flex-start; justify-content:space-between; gap:16px; padding:12px 0; border-bottom:1px solid var(--cf-divider);",
            div { style: "min-width:0;",
                div { style: "font-size:13px; font-weight:600;", "{title}" }
                div { style: "font-size:11px; color:var(--cf-text-muted); margin-top:2px; line-height:1.5;", "{desc}" }
            }
            div { style: "flex-shrink:0;", {control} }
        }
    }
}

fn commit_label(commit_hash: &Option<String>) -> String {
    match commit_hash {
        Some(hash) if !hash.is_empty() => hash.chars().take(12).collect(),
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
