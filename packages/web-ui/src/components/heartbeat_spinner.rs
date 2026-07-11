//! Heartbeat countdown ring/spinner component.
//!
//! During an active deployment the spinner borrows the same ring instead of
//! swapping in a different widget.  Each deploy stage has an expected duration
//! (based on the agent's pull cadence, not the fleet-wide heartbeat interval),
//! so the ring drains across that stage and the label counts down to the next
//! transition — exactly matching the updated design reference.
//!
//! Stage timing (mirrors JS DEPLOY_STAGE_*_MS constants):
//!   queued    0 → 15 s  (agent checks in on its next heartbeat)
//!   picked_up 15 → 17.2 s
//!   applying  17.2 s onward (unknown end — ring reverts to normal HB countdown)
//!   activated — ring fills solid green

use dioxus::prelude::*;

// Stage timing in seconds (mirroring the JS DEPLOY_STAGE_*_MS / 1000).
const STAGE_START: &[(&str, f64)] = &[
    ("queued", 0.0),
    ("picked_up", 15.0),
    ("applying", 17.2),
    ("activated", 20.6),
];
const STAGE_END: &[(&str, f64)] = &[
    ("queued", 15.0),
    ("picked_up", 17.2),
    ("applying", 20.6),
    ("activated", 24.3),
];

fn stage_start(stage: &str) -> f64 {
    STAGE_START
        .iter()
        .find(|(s, _)| *s == stage)
        .map(|(_, v)| *v)
        .unwrap_or(0.0)
}
fn stage_end(stage: &str) -> f64 {
    STAGE_END
        .iter()
        .find(|(s, _)| *s == stage)
        .map(|(_, v)| *v)
        .unwrap_or(stage_start(stage))
}

/// Animated heartbeat spinner that counts down to next expected heartbeat.
///
/// Pass `deploy_stage` and `deploy_started_at_ms` (JS epoch ms) while a
/// deployment is in progress; the ring turns purple and shows per-stage
/// countdown text.  Pass `None` for normal idle display.
#[component]
pub fn HeartbeatSpinner(
    interval_sec: i64,
    next_in_sec: f64,
    #[props(default = 36)] size: i32,
    #[props(default = true)] show_label: bool,
    #[props(default = None)] deploy_stage: Option<String>,
    #[props(default = None)] deploy_started_at_ms: Option<f64>,
) -> Element {
    let mounted_at = js_sys::Date::now() / 1000.0;
    let mut now_secs = use_signal(|| js_sys::Date::now() / 1000.0);

    use_future(move || async move {
        loop {
            gloo_timers::future::sleep(std::time::Duration::from_secs(1)).await;
            now_secs.set(js_sys::Date::now() / 1000.0);
        }
    });

    let now = now_secs();
    let elapsed = now - mounted_at;
    let remaining = next_in_sec - elapsed;
    let late_by = -remaining;

    let interval = interval_sec.max(1) as f64;
    let since_last = interval - remaining;
    let progress = (since_last / interval).clamp(0.0, 1.0);
    let overdue = remaining < 0.0;
    let critical = late_by > interval;

    // ── deploy-stage logic ──────────────────────────────────────────────────
    let stage = deploy_stage.as_deref().unwrap_or("");
    let activated = stage == "activated";
    // "applying" is open-ended — agent heartbeats keep flowing so the ring
    // reverts to the normal HB countdown; we just show elapsed time.
    let applying = stage == "applying";
    let deploying = !stage.is_empty() && !activated && !applying;

    // Elapsed seconds since deploy was first queued.
    let deploy_elapsed_secs = deploy_started_at_ms
        .map(|ms| (now - ms / 1000.0).max(0.0))
        .unwrap_or(0.0);

    // Per-stage countdown (queued / picked_up).
    let (stage_progress, stage_remain_sec) = if deploying {
        let s_start = stage_start(stage);
        let s_end = stage_end(stage);
        let dur = (s_end - s_start).max(0.001);
        let elapsed_in_stage = (deploy_elapsed_secs - s_start).max(0.0);
        let prog = (elapsed_in_stage / dur).clamp(0.0, 1.0);
        let remain = (dur - elapsed_in_stage).max(0.0);
        (prog, remain)
    } else {
        (0.0, 0.0)
    };

    // Elapsed inside the applying phase (counts UP).
    let applying_elapsed_sec = if applying {
        (deploy_elapsed_secs - stage_start("applying")).max(0.0)
    } else {
        0.0
    };

    // ── visual ─────────────────────────────────────────────────────────────
    let color = if activated {
        "#34d399"
    } else if deploying {
        "var(--cf-brand-purple)"
    } else if critical {
        "#f87171"
    } else if overdue {
        "#fbbf24"
    } else {
        "#34d399"
    };

    let stroke = ((size as f64) / 12.0).round().max(2.0);
    let radius = ((size as f64) - stroke) / 2.0;
    let circumference = 2.0 * std::f64::consts::PI * radius;

    // Ring offset:
    //   activated  → 0 (full ring, green)
    //   deploying  → drains as stage_progress goes 0→1
    //   applying   → normal HB countdown
    //   idle       → normal HB countdown (fills when overdue)
    let dash_offset = if activated {
        0.0
    } else if deploying {
        circumference * stage_progress
    } else if overdue {
        0.0
    } else {
        circumference * (1.0 - progress)
    };

    // No smooth transition during active deploy stages (mirrors JS design).
    let transition = if overdue || deploying {
        "none"
    } else {
        "stroke-dashoffset 0.8s linear, stroke 0.3s"
    };

    // ── labels ──────────────────────────────────────────────────────────────
    let label = if activated {
        "activated".to_string()
    } else if stage == "queued" {
        format!("picks up in {}", fmt_duration(stage_remain_sec))
    } else if stage == "picked_up" {
        format!("applying in {}", fmt_duration(stage_remain_sec))
    } else if overdue {
        format!("{} late", fmt_duration(late_by))
    } else {
        format!("next in {}", fmt_duration(remaining))
    };

    let sub = if activated {
        "generation live".to_string()
    } else if applying {
        format!("applying · {} elapsed", fmt_duration(applying_elapsed_sec))
    } else if deploying {
        "deploy in progress".to_string()
    } else {
        format!("every {}", fmt_duration(interval))
    };

    let ring_class = if critical {
        "hb-ring hb-overdue hb-critical"
    } else if overdue {
        "hb-ring hb-overdue"
    } else {
        "hb-ring"
    };

    rsx! {
        div {
            class: "hb-spinner",
            title: "Heartbeat {label} · {sub}",
            div {
                class: "{ring_class}",
                style: "width: {size}px; height: {size}px;",
                svg {
                    width: "{size}",
                    height: "{size}",
                    view_box: "0 0 {size} {size}",
                    circle {
                        cx: "{(size as f64) / 2.0}",
                        cy: "{(size as f64) / 2.0}",
                        r: "{radius}",
                        stroke: "rgba(148,163,184,0.18)",
                        stroke_width: "{stroke}",
                        fill: "none",
                    }
                    circle {
                        cx: "{(size as f64) / 2.0}",
                        cy: "{(size as f64) / 2.0}",
                        r: "{radius}",
                        stroke: "{color}",
                        stroke_width: "{stroke}",
                        fill: "none",
                        stroke_linecap: "round",
                        stroke_dasharray: "{circumference}",
                        stroke_dashoffset: "{dash_offset}",
                        transform: "rotate(-90 {(size as f64) / 2.0} {(size as f64) / 2.0})",
                        style: "transition: {transition};",
                    }
                }
                span {
                    class: "hb-pulse",
                    style: "background: {color};"
                }
            }
            if show_label {
                div {
                    class: "hb-label",
                    div {
                        class: "hb-label-main",
                        style: "color: {color};",
                        "{label}"
                    }
                    div {
                        class: "hb-label-sub",
                        "{sub}"
                    }
                }
            }
        }
    }
}

fn fmt_duration(seconds: f64) -> String {
    let value = seconds.abs().round() as i64;
    if value < 60 {
        format!("{}s", value)
    } else if value < 3600 {
        format!("{}m {}s", value / 60, value % 60)
    } else {
        format!("{}h {}m", value / 3600, (value % 3600) / 60)
    }
}
