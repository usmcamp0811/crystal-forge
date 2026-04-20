//! Heartbeat countdown ring/spinner component.

use dioxus::prelude::*;

/// Animated heartbeat spinner that counts down to next expected heartbeat.
#[component]
pub fn HeartbeatSpinner(
    interval_sec: i64,
    next_in_sec: f64,
    #[props(default = 36)] size: i32,
    #[props(default = true)] show_label: bool,
) -> Element {
    let mounted_at = js_sys::Date::now() / 1000.0;
    let mut now_secs = use_signal(|| js_sys::Date::now() / 1000.0);

    use_future(move || async move {
        loop {
            gloo_timers::future::sleep(std::time::Duration::from_secs(1)).await;
            now_secs.set(js_sys::Date::now() / 1000.0);
        }
    });

    let elapsed = now_secs() - mounted_at;
    let remaining = next_in_sec - elapsed;
    let late_by = -remaining;

    let interval = interval_sec.max(1) as f64;
    let since_last = interval - remaining;
    let progress = (since_last / interval).clamp(0.0, 1.0);

    let overdue = remaining < 0.0;
    let critical = late_by > interval;
    let color = if critical {
        "#f87171"
    } else if overdue {
        "#fbbf24"
    } else {
        "#34d399"
    };

    let stroke = ((size as f64) / 12.0).round().max(2.0);
    let radius = ((size as f64) - stroke) / 2.0;
    let circumference = 2.0 * std::f64::consts::PI * radius;
    let dash_offset = if overdue {
        0.0
    } else {
        circumference * (1.0 - progress)
    };

    let label = if overdue {
        format!("{} late", fmt_duration(late_by))
    } else {
        format!("next in {}", fmt_duration(remaining))
    };
    let sub = format!("every {}", fmt_duration(interval));

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
                        style: if overdue {
                            "transition: none;"
                        } else {
                            "transition: stroke-dashoffset 0.8s linear, stroke 0.3s;"
                        },
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
