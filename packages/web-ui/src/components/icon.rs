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
    ChevronUp,
    Grip,
    ArrowRight,
    Check,
    Search,
    Grid,
    Rows,
    Cpu,
    Gear,
    Warn,
    ArrowLeft,
    Rollback,
    /// Pencil / edit icon.
    Edit,
    /// Trash / delete icon.
    Trash,
    /// System detail tab icons (match CrystalForgelatest Icon.jsx paths).
    Dashboard,
    Server,
    Deploy,
    History,
    Key,
    File,
    Link,
    /// User / person icon (design Icon.jsx "user").
    User,
    /// Power button icon for system restarts (design Icon.jsx "power").
    Power,
    /// Clock / history-time icon (design Icon.jsx "history").
    Clock,
}

#[component]
pub fn Icon(name: IconName, #[props(default = 16)] size: u32) -> Element {
    let svg_content = match name {
        IconName::Sync => rsx! {
            path {
                d: "M20 12a8 8 0 0 1-14 5.3L3 14m1-4a8 8 0 0 1 14-5.3L21 8",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            path {
                d: "M21 3v5h-5M3 21v-5h5",
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
        IconName::ChevronUp => rsx! {
            path {
                d: "m18 15-6-6-6 6",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        IconName::Grip => rsx! {
            circle { cx: "9",  cy: "6",  r: "1.3", fill: "currentColor", stroke: "none" }
            circle { cx: "15", cy: "6",  r: "1.3", fill: "currentColor", stroke: "none" }
            circle { cx: "9",  cy: "12", r: "1.3", fill: "currentColor", stroke: "none" }
            circle { cx: "15", cy: "12", r: "1.3", fill: "currentColor", stroke: "none" }
            circle { cx: "9",  cy: "18", r: "1.3", fill: "currentColor", stroke: "none" }
            circle { cx: "15", cy: "18", r: "1.3", fill: "currentColor", stroke: "none" }
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
        IconName::ArrowLeft => rsx! {
            path {
                d: "M19 12H5M11 19l-7-7 7-7",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        IconName::Rollback => rsx! {
            path {
                d: "M3 7h11a6 6 0 1 1 0 12H8",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            path {
                d: "m8 3-5 4 5 4",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        // Pencil / edit (design Icon.jsx "edit").
        IconName::Edit => rsx! {
            path {
                d: "M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            path {
                d: "M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        // Trash / delete (design Icon.jsx "trash").
        IconName::Trash => rsx! {
            polyline {
                points: "3 6 5 6 21 6",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            path {
                d: "M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            path {
                d: "M10 11v6M14 11v6",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            path {
                d: "M9 6V4h6v2",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        // Dashboard: four rects laid out as a panel grid (design Icon.jsx "dashboard").
        IconName::Dashboard => rsx! {
            rect { x: "3", y: "3", width: "7", height: "9", rx: "1" }
            rect { x: "14", y: "3", width: "7", height: "5", rx: "1" }
            rect { x: "14", y: "12", width: "7", height: "9", rx: "1" }
            rect { x: "3", y: "16", width: "7", height: "5", rx: "1" }
        },
        // Server rack (design Icon.jsx "server").
        IconName::Server => rsx! {
            rect { x: "3", y: "4", width: "18", height: "7", rx: "1.5" }
            rect { x: "3", y: "13", width: "18", height: "7", rx: "1.5" }
            circle { cx: "7", cy: "7.5", r: "0.6", fill: "currentColor", stroke: "none" }
            circle { cx: "7", cy: "16.5", r: "0.6", fill: "currentColor", stroke: "none" }
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
        // Chain link (design Icon.jsx "link").
        IconName::Link => rsx! {
            path {
                d: "M10 14a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            path {
                d: "M14 10a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        // User / person (design Icon.jsx "user").
        IconName::User => rsx! {
            path {
                d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            circle { cx: "12", cy: "7", r: "4" }
        },
        // Power button for system restarts (design Icon.jsx "power").
        IconName::Power => rsx! {
            path {
                d: "M18.36 6.64a9 9 0 1 1-12.73 0",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
            path {
                d: "M12 2v10",
                stroke_linecap: "round",
                stroke_linejoin: "round"
            }
        },
        // Clock / history-time (design Icon.jsx "history").
        IconName::Clock => rsx! {
            circle { cx: "12", cy: "12", r: "9" }
            path {
                d: "M12 7v5l3 2",
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
