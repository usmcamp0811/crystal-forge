//! Draggable and resizable widget grid for dashboard layouts.

use dioxus::prelude::*;

use crate::theme;

/// Position and size of a widget in the grid.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct WidgetLayout {
    /// Column start (0-based)
    pub col: usize,
    /// Row start (0-based)
    pub row: usize,
    /// Width in grid columns
    pub width: usize,
    /// Height in grid rows
    pub height: usize,
}

impl WidgetLayout {
    pub fn new(col: usize, row: usize, width: usize, height: usize) -> Self {
        Self {
            col,
            row,
            width,
            height,
        }
    }
}

/// Definition of a widget for the grid
#[derive(Clone, PartialEq)]
pub struct WidgetDef {
    pub id: &'static str,
    pub title: &'static str,
    pub default_col: usize,
    pub default_row: usize,
    pub width: usize,
    pub height: usize,
}

/// Props for a single grid widget wrapper.
#[derive(Props, Clone, PartialEq)]
pub struct GridWidgetProps {
    pub id: String,
    pub title: String,
    pub col: usize,
    pub row: usize,
    pub width: usize,
    pub height: usize,
    pub children: Element,
    #[props(default = false)]
    pub is_dragging: bool,
    #[props(default = false)]
    pub is_drop_target: bool,
    #[props(default = false)]
    pub is_invalid_drop_target: bool,
    #[props(default)]
    pub on_drag_start: Option<EventHandler<String>>,
    #[props(default)]
    pub on_drag_over: Option<EventHandler<String>>,
    #[props(default)]
    pub on_drag_leave: Option<EventHandler<()>>,
    #[props(default)]
    pub on_drop: Option<EventHandler<String>>,
}

/// A single widget in the grid with drag handle.
#[component]
pub fn GridWidget(props: GridWidgetProps) -> Element {
    let GridWidgetProps {
        id,
        title,
        col,
        row,
        width,
        height,
        children,
        is_dragging,
        is_drop_target,
        is_invalid_drop_target,
        on_drag_start,
        on_drag_over,
        on_drag_leave,
        on_drop,
    } = props;

    // CSS grid positioning (1-based)
    let grid_col = format!("{} / span {}", col + 1, width);
    let grid_row = format!("{} / span {}", row + 1, height);

    // Visual states
    let drag_class = if is_dragging {
        "opacity-50 scale-105 shadow-2xl z-50"
    } else {
        ""
    };
    let drop_class = if is_drop_target {
        "ring-2 ring-blue-400 ring-offset-2 scale-[1.02] bg-blue-900/20"
    } else if is_invalid_drop_target {
        "ring-2 ring-red-400 ring-offset-2 bg-red-900/20"
    } else {
        ""
    };

    rsx! {
        div {
            class: "{theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} rounded-xl overflow-hidden transition-all duration-150 {drag_class} {drop_class}",
            style: "grid-column: {grid_col}; grid-row: {grid_row};",
            "data-widget-id": "{id}",
            draggable: "true",
            ondragstart: {
                let id = id.clone();
                let on_drag_start = on_drag_start.clone();
                move |_evt| {
                    if let Some(handler) = &on_drag_start {
                        handler.call(id.clone());
                    }
                }
            },
            ondragover: {
                let id = id.clone();
                let on_drag_over = on_drag_over.clone();
                move |evt| {
                    evt.prevent_default();
                    if let Some(handler) = &on_drag_over {
                        handler.call(id.clone());
                    }
                }
            },
            ondrop: {
                let id = id.clone();
                let on_drop = on_drop.clone();
                move |evt| {
                    evt.prevent_default();
                    if let Some(handler) = &on_drop {
                        handler.call(id.clone());
                    }
                }
            },
            ondragleave: {
                let on_drag_leave = on_drag_leave.clone();
                move |_| {
                    if let Some(handler) = &on_drag_leave {
                        handler.call(());
                    }
                }
            },

            // Header with drag handle
            div {
                class: "flex items-center gap-2 px-3 py-1.5 {theme::surface::SUBTLE_BG} border-b {theme::surface::CARD_BORDER} cursor-grab active:cursor-grabbing",

                // Drag handle icon (6-dot grip)
                svg {
                    width: "8",
                    height: "12",
                    view_box: "0 0 8 12",
                    class: "shrink-0",
                    circle { cx: "2", cy: "2", r: "1.2", fill: "#6b7280" }
                    circle { cx: "2", cy: "6", r: "1.2", fill: "#6b7280" }
                    circle { cx: "2", cy: "10", r: "1.2", fill: "#6b7280" }
                    circle { cx: "6", cy: "2", r: "1.2", fill: "#6b7280" }
                    circle { cx: "6", cy: "6", r: "1.2", fill: "#6b7280" }
                    circle { cx: "6", cy: "10", r: "1.2", fill: "#6b7280" }
                }
                h3 {
                    class: "{theme::text::PRIMARY} font-semibold text-sm whitespace-nowrap",
                    "{title}"
                }
            }

            // Widget content
            div {
                class: "p-4 h-full min-h-0 overflow-hidden",
                {children}
            }
        }
    }
}

/// Props for the widget grid container.
#[derive(Props, Clone, PartialEq)]
pub struct WidgetGridProps {
    /// Number of columns in the grid
    #[props(default = 4)]
    pub columns: usize,
    /// Gap between widgets in pixels
    #[props(default = 16)]
    pub gap: usize,
    /// Row height in pixels
    #[props(default = 100)]
    pub row_height: usize,
    /// Children (GridWidget components)
    pub children: Element,
}

/// Container for the widget grid.
#[component]
pub fn WidgetGrid(props: WidgetGridProps) -> Element {
    let WidgetGridProps {
        columns,
        gap,
        row_height,
        children,
    } = props;

    let grid_template = format!("repeat({}, minmax(0, 1fr))", columns);
    let grid_auto_rows = format!("{}px", row_height);
    let grid_gap = format!("{}px", gap);

    rsx! {
        div {
            class: "grid w-full",
            style: "grid-template-columns: {grid_template}; grid-auto-rows: {grid_auto_rows}; gap: {grid_gap};",
            {children}
        }
    }
}

/// Stored layout for persistence
#[derive(Clone, Debug, PartialEq, Default)]
pub struct StoredLayout {
    pub positions: Vec<(String, usize, usize)>, // (id, col, row)
}

impl StoredLayout {
    /// Load from localStorage
    pub fn load() -> Option<Self> {
        #[cfg(target_arch = "wasm32")]
        {
            let window = web_sys::window()?;
            let storage = window.local_storage().ok()??;
            let json = storage.get_item("dashboard_layout").ok()??;
            serde_json::from_str(&json).ok()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            None
        }
    }

    /// Save to localStorage
    pub fn save(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(json) = serde_json::to_string(self) {
                        let _ = storage.set_item("dashboard_layout", &json);
                    }
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl serde::Serialize for StoredLayout {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.positions.len()))?;
        for (id, col, row) in &self.positions {
            seq.serialize_element(&(id, col, row))?;
        }
        seq.end()
    }
}

#[cfg(target_arch = "wasm32")]
impl<'de> serde::Deserialize<'de> for StoredLayout {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let positions: Vec<(String, usize, usize)> = Vec::deserialize(deserializer)?;
        Ok(StoredLayout { positions })
    }
}
