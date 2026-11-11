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
//!
//! # Shared clock
//!
//! Every spinner reads one [`HeartbeatClock`] that the app shell provides with
//! [`use_heartbeat_clock_provider`]. The clock owns the only timer. It runs
//! while at least one spinner is mounted and the document is visible, so the
//! number of timers does not grow with the number of spinners. It resumes with
//! the current time when the document becomes visible again. A spinner rendered
//! without a provider shows a static snapshot instead of ticking.

use crate::hooks::visibility;
use dioxus::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

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

/// Counts the spinners that need the shared clock.
///
/// The clock timer exists only while the count is nonzero. Acquiring while
/// other spinners already hold the clock starts nothing.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ClockDemand {
    consumers: usize,
}

impl ClockDemand {
    /// Registers one spinner. Returns true when this is the first one, which is
    /// the only case that starts the timer.
    pub(crate) fn acquire(&mut self) -> bool {
        self.consumers += 1;
        self.consumers == 1
    }

    /// Unregisters one spinner. Returns true when none remain, which stops the
    /// timer. Releasing more often than acquiring cannot underflow.
    pub(crate) fn release(&mut self) -> bool {
        self.consumers = self.consumers.saturating_sub(1);
        self.consumers == 0
    }

    /// Returns whether any spinner needs the clock.
    pub(crate) fn active(&self) -> bool {
        self.consumers > 0
    }
}

/// Provides whole-second wall-clock time to every heartbeat spinner.
#[derive(Clone, Copy)]
pub struct HeartbeatClock {
    now: Signal<f64>,
    demand: Signal<ClockDemand>,
}

/// Returns the wall clock in seconds since the Unix epoch.
fn wall_clock_secs() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() / 1000.0
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs_f64())
            .unwrap_or(0.0)
    }
}

/// Quantizes to the second that the labels display, so identical readings never
/// produce a state update.
fn whole_secs(seconds: f64) -> f64 {
    seconds.floor()
}

/// Provides the shared heartbeat clock to the components below the caller.
///
/// Call once, in a component that outlives every page, such as the app shell.
/// The timer is a single reactive task: it starts when the first spinner
/// registers, stops when the last one leaves, and, while the document is
/// hidden, holds no timer and writes nothing. After a hidden period the first
/// write carries the current time.
pub fn use_heartbeat_clock_provider() {
    let mut now = use_signal(|| whole_secs(wall_clock_secs()));
    let demand = use_signal(ClockDemand::default);
    use_context_provider(|| HeartbeatClock { now, demand });
    // The future reads `demand` before its first await, so Dioxus restarts it
    // when the demand changes and drops the previous run, which cancels its timer.
    let _clock = use_resource(move || async move {
        if !demand.read().active() {
            return;
        }
        loop {
            let reading = whole_secs(wall_clock_secs());
            if *now.peek() != reading {
                now.set(reading);
            }
            visibility::visible_tick(1000).await;
        }
    });
}

/// Inputs of one countdown reading, independent of the browser.
pub(crate) struct HeartbeatInput<'a> {
    pub interval_sec: i64,
    /// Seconds until the next heartbeat when the value was received.
    pub next_in_sec: f64,
    /// Wall-clock second at which `next_in_sec` was received.
    pub anchor_secs: f64,
    pub size: i32,
    pub deploy_stage: Option<&'a str>,
    pub deploy_started_at_ms: Option<f64>,
}

/// Everything the ring and its labels show for one reading.
#[derive(Debug, PartialEq)]
pub(crate) struct HeartbeatDisplay {
    pub color: &'static str,
    pub dash_offset: f64,
    pub circumference: f64,
    pub radius: f64,
    pub stroke: f64,
    pub transition: &'static str,
    pub label: String,
    pub sub: String,
    pub ring_class: &'static str,
    pub overdue: bool,
    pub critical: bool,
}

/// Computes the display for wall-clock second `now`.
///
/// Elapsed time is measured from the anchor, never from the render, so a
/// re-render does not restart the countdown. `now` earlier than the anchor
/// counts as no time elapsed.
pub(crate) fn heartbeat_display(input: &HeartbeatInput<'_>, now: f64) -> HeartbeatDisplay {
    let elapsed = (now - input.anchor_secs).max(0.0);
    let remaining = input.next_in_sec - elapsed;
    let late_by = -remaining;

    let interval = input.interval_sec.max(1) as f64;
    let since_last = interval - remaining;
    let progress = (since_last / interval).clamp(0.0, 1.0);
    let overdue = remaining < 0.0;
    let critical = late_by > interval;

    // ── deploy-stage logic ──────────────────────────────────────────────────
    let stage = input.deploy_stage.unwrap_or("");
    let activated = stage == "activated";
    // "applying" is open-ended — agent heartbeats keep flowing so the ring
    // reverts to the normal HB countdown; we just show elapsed time.
    let applying = stage == "applying";
    let deploying = !stage.is_empty() && !activated && !applying;

    // Elapsed seconds since deploy was first queued.
    let deploy_elapsed_secs = input
        .deploy_started_at_ms
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

    let size = input.size;
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

    HeartbeatDisplay {
        color,
        dash_offset,
        circumference,
        radius,
        stroke,
        transition,
        label,
        sub,
        ring_class,
        overdue,
        critical,
    }
}

/// Animated heartbeat spinner that counts down to next expected heartbeat.
///
/// Pass `deploy_stage` and `deploy_started_at_ms` (JS epoch ms) while a
/// deployment is in progress; the ring turns purple and shows per-stage
/// countdown text.  Pass `None` for normal idle display.
///
/// `next_in_sec` is the number of seconds until the next heartbeat when the
/// caller computed it. The countdown runs from the moment this component first
/// received that value, and restarts when the caller passes a different one.
#[component]
pub fn HeartbeatSpinner(
    interval_sec: i64,
    next_in_sec: f64,
    #[props(default = 36)] size: i32,
    #[props(default = true)] show_label: bool,
    #[props(default = None)] deploy_stage: Option<String>,
    #[props(default = None)] deploy_started_at_ms: Option<f64>,
) -> Element {
    // The anchor is the wall-clock second at which the current `next_in_sec`
    // arrived. It survives re-renders, so a render never restarts the countdown.
    let anchor = use_hook(|| Rc::new(Cell::new((next_in_sec, whole_secs(wall_clock_secs())))));
    if anchor.get().0 != next_in_sec {
        anchor.set((next_in_sec, whole_secs(wall_clock_secs())));
    }
    let anchor_secs = anchor.get().1;

    let clock = use_hook(try_consume_context::<HeartbeatClock>);
    use_effect(move || {
        if let Some(clock) = clock {
            let mut demand = clock.demand;
            demand.write().acquire();
        }
    });
    use_drop(move || {
        if let Some(clock) = clock {
            // The provider can be gone first when the whole app unmounts.
            let mut demand = clock.demand;
            if let Ok(mut demand) = demand.try_write() {
                demand.release();
            }
        }
    });

    let now = match clock {
        Some(clock) => (clock.now)(),
        None => whole_secs(wall_clock_secs()),
    };
    let display = heartbeat_display(
        &HeartbeatInput {
            interval_sec,
            next_in_sec,
            anchor_secs,
            size,
            deploy_stage: deploy_stage.as_deref(),
            deploy_started_at_ms,
        },
        now,
    );
    let HeartbeatDisplay {
        color,
        dash_offset,
        circumference,
        radius,
        stroke,
        transition,
        label,
        sub,
        ring_class,
        ..
    } = display;

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

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: f64 = 1_000_000.0;

    fn input(interval_sec: i64, next_in_sec: f64) -> HeartbeatInput<'static> {
        HeartbeatInput {
            interval_sec,
            next_in_sec,
            anchor_secs: NOW,
            size: 36,
            deploy_stage: None,
            deploy_started_at_ms: None,
        }
    }

    #[test]
    fn countdown_advances_from_the_anchor_and_not_from_the_render() {
        let reading = input(30, 20.0);
        assert_eq!(heartbeat_display(&reading, NOW).label, "next in 20s");
        assert_eq!(heartbeat_display(&reading, NOW + 5.0).label, "next in 15s");
        // Rendering again at the same instant gives the same reading. The old
        // per-render `mounted_at` made every render report zero elapsed time.
        assert_eq!(
            heartbeat_display(&reading, NOW + 5.0),
            heartbeat_display(&reading, NOW + 5.0)
        );
        // A clock reading older than the anchor means no time has elapsed.
        assert_eq!(heartbeat_display(&reading, NOW - 3.0).label, "next in 20s");
    }

    #[test]
    fn overdue_and_critical_transitions_use_exact_boundaries() {
        let reading = input(30, 10.0);
        let at = |seconds: f64| heartbeat_display(&reading, NOW + seconds);
        // Exactly due is not yet overdue.
        let due = at(10.0);
        assert!(!due.overdue && !due.critical);
        assert_eq!(due.label, "next in 0s");
        assert_eq!(due.color, "#34d399");
        // One second past due is overdue, not critical.
        let late = at(11.0);
        assert!(late.overdue && !late.critical);
        assert_eq!(late.label, "1s late");
        assert_eq!(late.color, "#fbbf24");
        assert_eq!(late.ring_class, "hb-ring hb-overdue");
        assert_eq!(late.transition, "none");
        assert_eq!(late.dash_offset, 0.0, "an overdue ring is full");
        // Late by exactly one interval is overdue; later than that is critical.
        let one_interval = at(40.0);
        assert!(one_interval.overdue && !one_interval.critical);
        let critical = at(41.0);
        assert!(critical.overdue && critical.critical);
        assert_eq!(critical.label, "31s late");
        assert_eq!(critical.color, "#f87171");
        assert_eq!(critical.ring_class, "hb-ring hb-overdue hb-critical");
    }

    #[test]
    fn ring_drains_with_progress_and_clamps() {
        let reading = input(30, 30.0);
        let full = heartbeat_display(&reading, NOW);
        assert_eq!(
            full.dash_offset, full.circumference,
            "just beat: ring empty"
        );
        let half = heartbeat_display(&reading, NOW + 15.0);
        assert!((half.dash_offset - half.circumference / 2.0).abs() < 1e-9);
        // A value larger than the interval clamps instead of overflowing.
        let early = heartbeat_display(&input(30, 90.0), NOW);
        assert_eq!(early.dash_offset, early.circumference);
        assert_eq!(heartbeat_display(&input(0, 5.0), NOW).sub, "every 1s");
    }

    #[test]
    fn deploy_stages_keep_their_countdowns() {
        let started_ms = NOW * 1000.0;
        let at = |stage: &'static str, seconds: f64| {
            heartbeat_display(
                &HeartbeatInput {
                    deploy_stage: Some(stage),
                    deploy_started_at_ms: Some(started_ms),
                    ..input(30, 25.0)
                },
                NOW + seconds,
            )
        };
        let queued = at("queued", 5.0);
        assert_eq!(queued.label, "picks up in 10s");
        assert_eq!(queued.sub, "deploy in progress");
        assert_eq!(queued.color, "var(--cf-brand-purple)");
        assert_eq!(queued.transition, "none");
        assert_eq!(at("picked_up", 16.0).label, "applying in 1s");
        let applying = at("applying", 27.2);
        assert_eq!(applying.sub, "applying · 10s elapsed");
        // Applying reverts to the heartbeat countdown label: 27.2 s elapsed
        // against 25 s remaining is 2.2 s late, shown rounded.
        assert_eq!(applying.label, "2s late");
        let activated = at("activated", 30.0);
        assert_eq!(activated.label, "activated");
        assert_eq!(activated.sub, "generation live");
        assert_eq!(activated.dash_offset, 0.0);
        assert_eq!(activated.color, "#34d399");
    }

    #[test]
    fn durations_format_like_the_original() {
        assert_eq!(fmt_duration(59.4), "59s");
        assert_eq!(fmt_duration(60.0), "1m 0s");
        assert_eq!(fmt_duration(3_599.0), "59m 59s");
        assert_eq!(fmt_duration(3_600.0), "1h 0m");
        assert_eq!(fmt_duration(-12.0), "12s");
    }

    #[test]
    fn many_spinners_start_one_timer_and_the_last_one_stops_it() {
        let mut demand = ClockDemand::default();
        assert!(!demand.active());
        // The Systems page mounts dozens of spinners: only the first starts the
        // shared timer.
        let starts = (0..60).filter(|_| demand.acquire()).count();
        assert_eq!(starts, 1);
        assert!(demand.active());
        // Only the last release stops it.
        let stops = (0..60).filter(|_| demand.release()).count();
        assert_eq!(stops, 1);
        assert!(!demand.active());
        // Releasing without a holder is harmless and the next acquire restarts.
        assert!(demand.release());
        assert!(demand.acquire());
    }

    #[test]
    fn clock_readings_are_quantized_to_displayed_seconds() {
        assert_eq!(whole_secs(1_000.0), whole_secs(1_000.999));
        assert_ne!(whole_secs(1_000.999), whole_secs(1_001.0));
    }
}
