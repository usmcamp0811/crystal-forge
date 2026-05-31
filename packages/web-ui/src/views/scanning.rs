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
                        div { class: "dash-w-body", style: "gap:8px;",
                            div { style: "font-size:12px;", strong { "Scan started" } " · campground-config-main" }
                            div { style: "font-size:12px;", strong { "Scan completed" } " · ops-jumpbox" }
                            div { style: "font-size:12px;", strong { "Scan failed" } " · legacy-edge-node" }
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
                        div { class: "modal-head", h2 { "Scan schedule" }, p { "Control how often vulnix rescans configurations." } }
                        div { class: "modal-body", p { class: "page-subtitle", "Schedule controls mirror the ScanningView design and will be wired to backend policy next." } }
                        div {
                            class: "modal-foot",
                            button { class: "btn btn-ghost focus-ring", onclick: move |_| schedule_open.set(false), "Cancel" }
                            button { class: "btn btn-primary focus-ring", onclick: move |_| schedule_open.set(false), "Save" }
                        }
                    }
                }
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
