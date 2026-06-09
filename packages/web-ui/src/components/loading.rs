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
    let stroke = ((size as f64) / 10.0).round().max(2.0);
    let radius = ((size as f64) - stroke) / 2.0;
    let circumference = 2.0 * std::f64::consts::PI * radius;
    let arc = circumference * 0.72;
    let dash_array = format!("{} {}", arc, circumference);
    let color = "var(--cf-brand-purple)";

    rsx! {
        div {
            class: "hb-spinner",
            div {
                class: "hb-ring",
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
                        stroke_dasharray: "{dash_array}",
                        stroke_dashoffset: "{circumference * 0.18}",
                        transform: "rotate(-90 {(size as f64) / 2.0} {(size as f64) / 2.0})",
                        class: "cf-spinner-ring",
                    }
                }
                span {
                    class: "hb-pulse",
                    style: "background: {color};"
                }
            }
            div {
                class: "hb-label",
                div {
                    class: "hb-label-main",
                    style: "color: {color};",
                    "{label}"
                }
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
