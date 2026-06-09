//! Loading spinner and error display components.

use dioxus::prelude::*;

/// A centered loading spinner.
#[component]
pub fn LoadingSpinner() -> Element {
    rsx! {
        div {
            class: "flex items-center justify-center p-12",
            div {
                class: "animate-spin rounded-full h-8 w-8 border-b-2 border-blue-400",
            }
        }
    }
}

/// Enhanced loading spinner with label - design reference inspired.
#[component]
pub fn DashboardLoadingSpinner(
    #[props(default = "Loading...".to_string())] label: String,
    #[props(default = 20)] size: i32,
) -> Element {
    let stroke = ((size as f64) / 8.0).round().max(2.0);
    let radius = ((size as f64) - stroke) / 2.0;
    let circumference = 2.0 * std::f64::consts::PI * radius;
    let gradient_id = format!(
        "cf-spinner-gradient-{}",
        label
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() {
                    ch.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
    );
    let gradient_url = format!("url(#{gradient_id})");

    rsx! {
        div {
            class: "cf-loading-row",
            div {
                class: "cf-loading-ring",
                style: "width: {size}px; height: {size}px;",
                svg {
                    width: "{size}",
                    height: "{size}",
                    view_box: "0 0 {size} {size}",
                    circle {
                        cx: "{(size as f64) / 2.0}",
                        cy: "{(size as f64) / 2.0}",
                        r: "{radius}",
                        stroke: "rgba(167, 139, 250, 0.25)",
                        stroke_width: "{stroke}",
                        fill: "none",
                    }
                    circle {
                        cx: "{(size as f64) / 2.0}",
                        cy: "{(size as f64) / 2.0}",
                        r: "{radius}",
                        stroke: "{gradient_url}",
                        stroke_width: "{stroke}",
                        fill: "none",
                        stroke_linecap: "round",
                        stroke_dasharray: "{circumference}",
                        stroke_dashoffset: "{circumference * 0.25}",
                        transform: "rotate(-90 {(size as f64) / 2.0} {(size as f64) / 2.0})",
                        class: "cf-spinner-ring",
                    }
                    defs {
                        linearGradient {
                            id: "{gradient_id}",
                            x1: "0%",
                            y1: "0%",
                            x2: "100%",
                            y2: "100%",
                            stop {
                                offset: "0%",
                                stop_color: "#a78bfa",
                                stop_opacity: "1",
                            }
                            stop {
                                offset: "100%",
                                stop_color: "#60a5fa",
                                stop_opacity: "0.9",
                            }
                        }
                    }
                }
            }
            p {
                class: "cf-loading-label",
                "{label}"
            }
        }
    }
}

/// An error message display.
#[component]
pub fn ErrorMessage(message: String) -> Element {
    rsx! {
        div {
            class: "bg-red-900/20 border border-red-800 rounded-lg p-4 text-red-400",
            p {
                class: "font-medium",
                "Error"
            }
            p {
                class: "text-sm mt-1",
                "{message}"
            }
        }
    }
}
