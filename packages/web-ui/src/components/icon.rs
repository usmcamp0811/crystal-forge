//! Simple icon component for inline SVG icons.

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    Sync,
    Plus,
    Terminal,
    X,
    Download,
    Shield,
    Git,
    ChevronRight,
    ChevronDown,
    ArrowRight,
    Check,
}

#[component]
pub fn Icon(name: IconName, #[props(default = 16)] size: u32) -> Element {
    let svg_content = match name {
        IconName::Sync => rsx! {
            path {
                d: "M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        IconName::Plus => rsx! {
            path {
                d: "M5 12h14M12 5v14",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        IconName::Terminal => rsx! {
            polyline {
                points: "4 17 10 11 4 5",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            line {
                x1: "12",
                x2: "20",
                y1: "19",
                y2: "19",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        IconName::X => rsx! {
            path {
                d: "M18 6 6 18M6 6l12 12",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        IconName::Download => rsx! {
            path {
                d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        IconName::Shield => rsx! {
            path {
                d: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        IconName::Git => rsx! {
            circle { cx: "12", cy: "18", r: "3" }
            circle { cx: "6", cy: "6", r: "3" }
            circle { cx: "18", cy: "6", r: "3" }
            path {
                d: "M18 9a9 9 0 0 1-9 9M9 6h6",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        IconName::ChevronRight => rsx! {
            path {
                d: "m9 18 6-6-6-6",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        IconName::ChevronDown => rsx! {
            path {
                d: "m6 9 6 6 6-6",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        IconName::ArrowRight => rsx! {
            path {
                d: "M5 12h14M12 5l7 7-7 7",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        IconName::Check => rsx! {
            path {
                d: "M20 6 9 17l-5-5",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
    };

    rsx! {
        svg {
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            {svg_content}
        }
    }
}
