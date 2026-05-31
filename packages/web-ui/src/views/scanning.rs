use dioxus::prelude::*;

use crate::routes::Route;

#[component]
pub fn ScanningView() -> Element {
    let nav = navigator();
    let mut tab = use_signal(|| "queue".to_string());
    let mut show_activity = use_signal(|| true);
    let mut schedule_open = use_signal(|| false);

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:16px;",

            div {
                class: "page-head",
                div {
                    h1 { class: "page-title", "Scanning" }
                    p { class: "page-subtitle", "CVE scanning · vulnix 2.18.0 · DB updated 18h" }
                }
                div {
                    style: "display:flex; gap:8px;",
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| schedule_open.set(true),
                        "Schedule"
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        "Rescan all"
                    }
                }
            }

            div {
                class: "stat-strip",
                { stat_card("Scanning now", "3", Some("11 queued"), "#60a5fa") }
                { stat_card("Stale", "42", Some("past rescan interval"), "#fbbf24") }
                { stat_card("Never scanned", "5", None, "#9ca3af") }
                { stat_card("Failed", "1", None, "#f87171") }
                { stat_card("Coverage", "93%", Some("configs with results"), "#34d399") }
            }

            div {
                style: if show_activity() {
                    "display:grid; grid-template-columns: 1fr 320px; gap:14px; align-items:start;"
                } else {
                    "display:grid; grid-template-columns: 1fr; gap:14px; align-items:start;"
                },

                div {
                    class: "card",
                    style: "overflow:hidden;",

                    div {
                        class: "sd-tabs",
                        style: "padding:0 16px; border-bottom:1px solid var(--cf-card-border); display:flex; align-items:center;",
                        button {
                            class: if tab() == "queue" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            onclick: move |_| tab.set("queue".to_string()),
                            "Active & Recent"
                        }
                        button {
                            class: if tab() == "all" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            onclick: move |_| tab.set("all".to_string()),
                            "All configs"
                        }
                        if !show_activity() {
                            button {
                                class: "btn btn-ghost focus-ring xs",
                                style: "margin-left:auto;",
                                onclick: move |_| show_activity.set(true),
                                "Show activity"
                            }
                        }
                    }

                    if tab() == "queue" {
                        table {
                            class: "sys-table",
                            thead {
                                tr {
                                    th { "Config" }
                                    th { "Freshness" }
                                    th { "Status" }
                                    th { "Findings" }
                                    th { "Last scan" }
                                    th { "Trigger" }
                                    th { style: "text-align:right;", " " }
                                }
                            }
                            tbody {
                                tr {
                                    td {
                                        div { style: "font-weight:600; font-size:13px;", "campground-config-main" }
                                        div { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "github:acme/fleet · 8b6b6ebb" }
                                    }
                                    td { span { class: "chip chip-healthy", style: "font-size:10px;", "deployed" } }
                                    td {
                                        span { class: "chip chip-info", span { class: "chip-dot", style: "background:#60a5fa;" } "scanning" }
                                        div { style: "height:3px; margin-top:5px; background:var(--cf-subtle-bg); border-radius:99px; overflow:hidden; max-width:80px;",
                                            div { style: "width:62%; height:100%; background:#60a5fa;" }
                                        }
                                    }
                                    td {
                                        div { style: "display:flex; gap:4px;",
                                            span { class: "chip chip-critical", style: "font-size:10px;", "1C" }
                                            span { class: "chip chip-warning", style: "font-size:10px;", "4H" }
                                            span { class: "chip chip-info", style: "font-size:10px;", "12M" }
                                        }
                                    }
                                    td { style: "font-size:12px; color:var(--cf-text-muted);", "in progress" }
                                    td { span { class: "chip chip-unknown", style: "font-size:10px;", "schedule" } }
                                    td {
                                        div {
                                            class: "row-actions",
                                            button { class: "btn-icon focus-ring", title: "Rescan now", "↻" }
                                            button {
                                                class: "btn-icon focus-ring",
                                                title: "View CVEs",
                                                onclick: move |_| { nav.push(Route::CvesView {}); },
                                                "→"
                                            }
                                        }
                                    }
                                }
                                tr {
                                    td {
                                        div { style: "font-weight:600; font-size:13px;", "builder-image-stable" }
                                        div { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "github:acme/builders · 6f93aa62" }
                                    }
                                    td { span { class: "chip chip-info", style: "font-size:10px;", "recent" } }
                                    td {
                                        span { class: "chip chip-warning", span { class: "chip-dot", style: "background:#fbbf24;" } "queued" }
                                    }
                                    td {
                                        div { style: "display:flex; gap:4px;",
                                            span { class: "chip chip-info", style: "font-size:10px;", "3M" }
                                        }
                                    }
                                    td { style: "font-size:12px; color:var(--cf-text-muted);", "14m ago" }
                                    td { span { class: "chip chip-unknown", style: "font-size:10px;", "on-build" } }
                                    td {
                                        div { class: "row-actions", button { class: "btn-icon focus-ring", title: "Rescan now", "↻" } }
                                    }
                                }
                                tr {
                                    td {
                                        div { style: "font-weight:600; font-size:13px;", "legacy-edge-node" }
                                        div { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "github:acme/edge · 3dd052ac" }
                                    }
                                    td { span { class: "chip chip-unknown", style: "font-size:10px;", "archived" } }
                                    td {
                                        span { class: "chip chip-critical", span { class: "chip-dot", style: "background:#f87171;" } "failed" }
                                        div { style: "font-size:10px; color:#fca5a5; margin-top:3px;", "vulnix timed out" }
                                    }
                                    td { span { class: "chip chip-healthy", style: "font-size:10px;", "clean" } }
                                    td { style: "font-size:12px; color:var(--cf-text-muted);", "2h ago" }
                                    td { span { class: "chip chip-unknown", style: "font-size:10px;", "manual" } }
                                    td {
                                        div { class: "row-actions", button { class: "btn-icon focus-ring", title: "Retry scan", "↻" } }
                                    }
                                }
                            }
                        }
                    } else {
                        div {
                            style: "padding:10px 16px; border-top: 1px solid var(--cf-divider);",
                                div {
                                    style: "display:flex; align-items:center; gap:10px; flex-wrap:wrap; margin-bottom:10px;",
                                input {
                                    class: "input focus-ring",
                                    style: "max-width:220px;",
                                    placeholder: "Search systems…",
                                }
                                div {
                                    style: "display:flex; align-items:center; justify-content:space-between; gap:8px; margin-bottom:10px;",
                                    span { class: "page-subtitle", "Freshness bars: green=fresh · amber=stale · gray=never scanned" }
                                    button { class: "btn btn-ghost focus-ring xs", "Expand all" }
                                }
                                select {
                                    class: "input focus-ring",
                                    style: "width:auto;",
                                    option { value: "all", "All environments" }
                                    option { value: "lan", "LAN" }
                                    option { value: "wifi", "WiFi" }
                                }
                                span { class: "page-subtitle", "3 systems · 14 configs" }
                            }

                            table {
                                class: "sys-table",
                                thead {
                                    tr {
                                        th { "System" }
                                        th { "Env" }
                                        th { "Configs" }
                                        th { "Scan freshness" }
                                        th { "Current findings" }
                                        th { style: "text-align:right;", " " }
                                    }
                                }
                                tbody {
                                    tr {
                                        td {
                                            div { style: "font-weight:600; font-size:13px;", "butler" }
                                            div { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "github:acme/fleet" }
                                        }
                                        td { span { class: "chip chip-info", style: "font-size:10px;", "LAN" } }
                                        td { class: "mono", style: "font-size:12px;", "5" }
                                        td {
                                            div { style: "display:flex; align-items:center; gap:8px; min-width:120px;",
                                                div { style: "flex:1; height:5px; background:var(--cf-subtle-bg); border-radius:99px; overflow:hidden; display:flex;",
                                                    div { style: "width:60%; background:#34d399;" }
                                                    div { style: "width:20%; background:#fbbf24;" }
                                                    div { style: "width:20%; background:#4b5563;" }
                                                }
                                                span { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "3/5" }
                                            }
                                        }
                                        td {
                                            div { style: "display:flex; gap:4px;",
                                                span { class: "chip chip-critical", style: "font-size:10px;", "1C" }
                                                span { class: "chip chip-warning", style: "font-size:10px;", "2H" }
                                            }
                                        }
                                        td { div { class: "row-actions", button { class: "btn-icon focus-ring", "↻" } } }
                                    }
                                    tr {
                                        td {
                                            div { style: "font-weight:600; font-size:13px;", "gray" }
                                            div { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "github:acme/fleet" }
                                        }
                                        td { span { class: "chip chip-warning", style: "font-size:10px;", "WiFi" } }
                                        td { class: "mono", style: "font-size:12px;", "4" }
                                        td {
                                            div { style: "display:flex; align-items:center; gap:8px; min-width:120px;",
                                                div { style: "flex:1; height:5px; background:var(--cf-subtle-bg); border-radius:99px; overflow:hidden; display:flex;",
                                                    div { style: "width:75%; background:#34d399;" }
                                                    div { style: "width:25%; background:#fbbf24;" }
                                                }
                                                span { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "3/4" }
                                            }
                                        }
                                        td {
                                            div { style: "display:flex; gap:4px;",
                                                span { class: "chip chip-warning", style: "font-size:10px;", "1H" }
                                            }
                                        }
                                        td { div { class: "row-actions", button { class: "btn-icon focus-ring", "↻" } } }
                                    }
                                    tr {
                                        td {
                                            div { style: "font-weight:600; font-size:13px;", "ops-jumpbox" }
                                            div { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "github:acme/ops" }
                                        }
                                        td { span { class: "chip chip-critical", style: "font-size:10px;", "WAN" } }
                                        td { class: "mono", style: "font-size:12px;", "2" }
                                        td {
                                            div { style: "display:flex; align-items:center; gap:8px; min-width:120px;",
                                                div { style: "flex:1; height:5px; background:var(--cf-subtle-bg); border-radius:99px; overflow:hidden; display:flex;",
                                                    div { style: "width:50%; background:#34d399;" }
                                                    div { style: "width:50%; background:#4b5563;" }
                                                }
                                                span { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "1/2" }
                                            }
                                        }
                                        td {
                                            div { style: "display:flex; gap:4px;",
                                                span { class: "chip chip-healthy", style: "font-size:10px;", "clean" }
                                            }
                                        }
                                        td { div { class: "row-actions", button { class: "btn-icon focus-ring", "↻" } } }
                                    }
                                }
                            }
                        }
                    }
                }

                if show_activity() {
                    div {
                        class: "card",
                        style: "padding:16px;",
                        div {
                            style: "display:flex; align-items:center; justify-content:space-between; margin-bottom:12px;",
                            h3 { style: "margin:0; font-size:13px; font-weight:600;", "Scan activity" }
                            button {
                                class: "btn-icon focus-ring",
                                onclick: move |_| show_activity.set(false),
                                title: "Hide panel",
                                "×"
                            }
                        }
                        div {
                            class: "dash-w-body",
                            style: "gap:0;",
                            div {
                                style: "display:flex; gap:10px; padding-left:2px;",
                                div {
                                    style: "display:flex; flex-direction:column; align-items:center; padding-top:4px; flex-shrink:0;",
                                    div { style: "width:22px; height:22px; border-radius:6px; background:color-mix(in oklab, #60a5fa 18%, transparent); color:#60a5fa; display:grid; place-items:center; font-size:11px;", "↻" }
                                    div { style: "width:2px; flex:1; background:var(--cf-divider); min-height:16px;" }
                                }
                                div {
                                    style: "padding-top:3px; padding-bottom:14px; min-width:0;",
                                    div {
                                        style: "font-size:12px; color:var(--cf-text-primary); display:flex; gap:6px; justify-content:space-between;",
                                        span { style: "font-weight:600;", "Scan started" }
                                        span { style: "font-size:11px; color:var(--cf-text-muted); white-space:nowrap;", "just now" }
                                    }
                                    div { class: "mono", style: "font-size:11px; color:var(--cf-brand-purple);", "campground-config-main" }
                                    div { style: "font-size:11px; color:var(--cf-text-muted); margin-top:2px;", "Queued by schedule policy for deployed configs" }
                                }
                            }
                            div {
                                style: "display:flex; gap:10px; padding-left:2px;",
                                div {
                                    style: "display:flex; flex-direction:column; align-items:center; padding-top:4px; flex-shrink:0;",
                                    div { style: "width:22px; height:22px; border-radius:6px; background:color-mix(in oklab, #34d399 18%, transparent); color:#34d399; display:grid; place-items:center; font-size:11px;", "✓" }
                                    div { style: "width:2px; flex:1; background:var(--cf-divider); min-height:16px;" }
                                }
                                div {
                                    style: "padding-top:3px; padding-bottom:14px; min-width:0;",
                                    div {
                                        style: "font-size:12px; color:var(--cf-text-primary); display:flex; gap:6px; justify-content:space-between;",
                                        span { style: "font-weight:600;", "Scan completed" }
                                        span { style: "font-size:11px; color:var(--cf-text-muted); white-space:nowrap;", "3m ago" }
                                    }
                                    div { class: "mono", style: "font-size:11px; color:var(--cf-brand-purple);", "ops-jumpbox" }
                                    div { style: "font-size:11px; color:var(--cf-text-muted); margin-top:2px;", "No critical findings detected" }
                                }
                            }
                            div {
                                style: "display:flex; gap:10px; padding-left:2px;",
                                div {
                                    style: "display:flex; flex-direction:column; align-items:center; padding-top:4px; flex-shrink:0;",
                                    div { style: "width:22px; height:22px; border-radius:6px; background:color-mix(in oklab, #f87171 18%, transparent); color:#f87171; display:grid; place-items:center; font-size:11px;", "×" }
                                }
                                div {
                                    style: "padding-top:3px; padding-bottom:0; min-width:0;",
                                    div {
                                        style: "font-size:12px; color:var(--cf-text-primary); display:flex; gap:6px; justify-content:space-between;",
                                        span { style: "font-weight:600;", "Scan failed" }
                                        span { style: "font-size:11px; color:var(--cf-text-muted); white-space:nowrap;", "2h ago" }
                                    }
                                    div { class: "mono", style: "font-size:11px; color:var(--cf-brand-purple);", "legacy-edge-node" }
                                    div { style: "font-size:11px; color:var(--cf-text-muted); margin-top:2px;", "Timed out while resolving archived derivation" }
                                }
                            }
                        }
                    }
                }
            }

            if schedule_open() {
                div {
                    class: "modal-backdrop",
                    onclick: move |_| schedule_open.set(false),
                    div {
                        class: "modal",
                        style: "width:min(620px,96vw);",
                        onclick: move |evt| evt.stop_propagation(),
                        div { class: "modal-head", h2 { "Scan schedule" }, p { "Control how often vulnix rescans configurations. New & deployed configs scan most often; old ones least." } }
                        div {
                            class: "modal-body",
                            div {
                                style: "display:flex; align-items:flex-start; justify-content:space-between; gap:16px; padding:12px 0; border-bottom:1px solid var(--cf-divider);",
                                div {
                                    div { style: "font-size:13px; font-weight:600;", "Scan on build" }
                                    div { style: "font-size:11px; color:var(--cf-text-muted); margin-top:2px; line-height:1.5;", "Scan a freshly-built config before it can be deployed. Strongly recommended — the derivation is already in the store, so no extra build is needed." }
                                }
                                label { style: "display:flex; gap:8px; align-items:center; font-size:13px; cursor:pointer;",
                                    input { r#type: "checkbox", checked: true }
                                    span { "On" }
                                }
                            }
                            { schedule_interval_row(
                                "Deployed configs",
                                "Currently running on at least one system. Rescanned to catch newly-published advisories.",
                            ) }
                            { schedule_interval_row(
                                "Recent configs",
                                "Built in the last 30 days but not currently deployed.",
                            ) }
                            div {
                                style: "display:flex; align-items:flex-start; justify-content:space-between; gap:16px; padding:12px 0; border-bottom:1px solid var(--cf-divider);",
                                div {
                                    div { style: "font-size:13px; font-weight:600;", "Archived configs" }
                                    div { style: "font-size:11px; color:var(--cf-text-muted); margin-top:2px; line-height:1.5;", "Old / superseded configs no longer in rotation. Scan rarely (or never) to save builder time." }
                                }
                                div {
                                    style: "display:flex; align-items:center; gap:8px;",
                                    input { r#type: "checkbox", checked: true }
                                    select { class: "input focus-ring", style: "width:120px;", option { value: "168h", "Every 168h" } }
                                }
                            }
                            div {
                                style: "display:flex; align-items:flex-start; justify-content:space-between; gap:16px; padding:12px 0; border-bottom:1px solid var(--cf-divider);",
                                div {
                                    div { style: "font-size:13px; font-weight:600;", "Rebuild to scan old configs" }
                                    div { style: "font-size:11px; color:var(--cf-text-muted); margin-top:2px; line-height:1.5;", "vulnix needs a realised derivation. Archived configs evicted from cache must be rebuilt before they can be scanned — this can be expensive. Off = skip uncached configs instead of building them." }
                                }
                                label { style: "display:flex; gap:8px; align-items:center; font-size:13px; cursor:pointer;",
                                    input { r#type: "checkbox" }
                                    span { "Off" }
                                }
                            }
                            div {
                                class: "sd-callout sd-callout-info",
                                style: "font-size:11px; margin-top:12px;",
                                div { "Estimated load: ~every build scans + periodic rescans. Deployed configs at every 24h dominate builder cost." }
                            }
                        }
                        div {
                            class: "modal-foot",
                            button { class: "btn btn-ghost focus-ring", onclick: move |_| schedule_open.set(false), "Cancel" }
                            button { class: "btn btn-primary focus-ring", onclick: move |_| schedule_open.set(false), "Save schedule" }
                        }
                    }
                }
            }
        }
    }
}

fn schedule_interval_row(title: &'static str, desc: &'static str) -> Element {
    rsx! {
        div {
            style: "display:flex; align-items:flex-start; justify-content:space-between; gap:16px; padding:12px 0; border-bottom:1px solid var(--cf-divider);",
            div {
                style: "min-width:0;",
                div { style: "font-size:13px; font-weight:600;", "{title}" }
                div { style: "font-size:11px; color:var(--cf-text-muted); margin-top:2px; line-height:1.5;", "{desc}" }
            }
            select {
                class: "input focus-ring",
                style: "width:120px;",
                option { value: "24h", "Every 24h" }
                option { value: "168h", "Every 168h" }
                option { value: "never", "Never" }
            }
        }
    }
}

fn stat_card(label: &'static str, value: &'static str, meta: Option<&'static str>, color: &'static str) -> Element {
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
