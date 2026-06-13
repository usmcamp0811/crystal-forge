//! Simple icon component for inline SVG icons.

use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    Search,
    Grid,
    Rows,
    Cpu,
    Gear,
    Warn,
    /// System detail tab icons (match CrystalForgelatest Icon.jsx paths).
    Dashboard,
    Deploy,
    History,
    Key,
    File,
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
        IconName::Search => rsx! {
            circle { cx: "11", cy: "11", r: "8" }
            path {
                d: "m21 21-4.35-4.35",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        IconName::Grid => rsx! {
            rect { x: "3", y: "3", width: "7", height: "7", rx: "1" }
            rect { x: "14", y: "3", width: "7", height: "7", rx: "1" }
            rect { x: "14", y: "14", width: "7", height: "7", rx: "1" }
            rect { x: "3", y: "14", width: "7", height: "7", rx: "1" }
        },
        IconName::Rows => rsx! {
            rect { x: "3", y: "3", width: "18", height: "7", rx: "1" }
            rect { x: "3", y: "14", width: "18", height: "7", rx: "1" }
        },
        IconName::Cpu => rsx! {
            rect { x: "4", y: "4", width: "16", height: "16", rx: "2" }
            rect { x: "9", y: "9", width: "6", height: "6" }
            path { d: "M15 2v2M15 20v2M2 15h2M20 15h2M2 9h2M20 9h2M9 2v2M9 20v2" }
        },
        IconName::Gear => rsx! {
            path {
                d: "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            circle { cx: "12", cy: "12", r: "3" }
        },
        IconName::Warn => rsx! {
            path {
                d: "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            path { d: "M12 9v4M12 17h.01" }
        },
        // Dashboard: four rects laid out as a panel grid (design Icon.jsx "dashboard").
        IconName::Dashboard => rsx! {
            rect { x: "3", y: "3", width: "7", height: "9", rx: "1" }
            rect { x: "14", y: "3", width: "7", height: "5", rx: "1" }
            rect { x: "14", y: "12", width: "7", height: "9", rx: "1" }
            rect { x: "3", y: "16", width: "7", height: "5", rx: "1" }
        },
        // Deploy: upward arrow over a base tray (design Icon.jsx "deploy").
        IconName::Deploy => rsx! {
            path {
                d: "M12 3v12M6 9l6-6 6 6",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            rect { x: "4", y: "17", width: "16", height: "4", rx: "1" }
        },
        // History: clock-with-arrow (design Icon.jsx "history").
        IconName::History => rsx! {
            path {
                d: "M3 12a9 9 0 1 0 3-6.7L3 8",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            path {
                d: "M3 3v5h5M12 7v5l3 2",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        // Key (design Icon.jsx "key").
        IconName::Key => rsx! {
            circle { cx: "8", cy: "15", r: "4" }
            path {
                d: "m10.8 12.2 9-9M16 7l3 3",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        // File with folded corner (design Icon.jsx "file").
        IconName::File => rsx! {
            path {
                d: "M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9l-6-6z",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            path {
                d: "M14 3v6h6",
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
