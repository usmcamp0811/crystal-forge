//! Draggable, resizable dashboard widget grid.
//!
//! Ported to match the design reference (`CrystalForgelatest`): a 3-column
//! dense CSS grid where each widget spans `cols` (1-3) and `rows` (1-3), with
//! an edit toolbar exposing width/height segmented controls and a remove button.

use dioxus::prelude::*;

use crate::components::icon::{Icon, IconName};

/// Props for a single grid widget wrapper.
#[derive(Props, Clone, PartialEq)]
pub struct GridWidgetProps {
    pub id: String,
    pub title: String,
    /// Header icon.
    pub icon: IconName,
    /// Width in columns (1-3).
    pub cols: usize,
    /// Height in rows (1-3).
    pub rows: usize,
    /// Whether this widget supports height resizing (list/feed widgets).
    #[props(default = false)]
    pub height_resizable: bool,
    /// Optional "View →" header action label.
    #[props(default)]
    pub action_label: Option<String>,
    pub children: Element,
    #[props(default = false)]
    pub edit_mode: bool,
    #[props(default = false)]
    pub is_dragging: bool,
    #[props(default = false)]
    pub is_drop_target: bool,
    #[props(default = false)]
    pub is_invalid_drop_target: bool,
    #[props(default)]
    pub on_action: Option<EventHandler<()>>,
    #[props(default)]
    pub on_drag_start: Option<EventHandler<String>>,
    #[props(default)]
    pub on_drag_over: Option<EventHandler<String>>,
    #[props(default)]
    pub on_drag_leave: Option<EventHandler<()>>,
    #[props(default)]
    pub on_drop: Option<EventHandler<String>>,
    #[props(default)]
    pub on_set_cols: Option<EventHandler<(String, usize)>>,
    #[props(default)]
    pub on_set_rows: Option<EventHandler<(String, usize)>>,
    #[props(default)]
    pub on_remove: Option<EventHandler<String>>,
}

/// A single dashboard widget rendered with the design-reference markup.
#[component]
pub fn GridWidget(props: GridWidgetProps) -> Element {
    let GridWidgetProps {
        id,
        title,
        icon,
        cols,
        rows,
        height_resizable,
        action_label,
        children,
        edit_mode,
        is_dragging,
        is_drop_target,
        is_invalid_drop_target,
        on_action,
        on_drag_start,
        on_drag_over,
        on_drag_leave,
        on_drop,
        on_set_cols,
        on_set_rows,
        on_remove,
    } = props;

    let cols = cols.clamp(1, 3);
    let rows = rows.clamp(1, 3);

    let mut classes = format!("dash-widget dash-cols-{cols} dash-rows-{rows}");
    if edit_mode {
        classes.push_str(" edit");
    }
    if is_dragging {
        classes.push_str(" dragging");
    }
    if is_drop_target {
        classes.push_str(" over");
    } else if is_invalid_drop_target {
        classes.push_str(" invalid");
    }

    rsx! {
        div {
            class: "{classes}",
            "data-widget-id": "{id}",
            draggable: "{edit_mode}",
            ondragstart: {
                let id = id.clone();
                let on_drag_start = on_drag_start.clone();
                move |_| {
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

            // Edit toolbar (width / height / remove)
            if edit_mode {
                div {
                    class: "dash-widget-edit",
                    span { class: "dash-widget-grip", title: "Drag to move",
                        Icon { name: IconName::Rows, size: 12 }
                    }
                    span {
                        class: "dash-size-group",
                        span { class: "dash-col-label", "Width" }
                        div {
                            class: "seg dash-col-seg",
                            for c in 1..=3usize {
                                button {
                                    key: "w{c}",
                                    class: if cols == c { "active" } else { "" },
                                    title: "Span {c} of 3 columns",
                                    "aria-label": "Span {c} of 3 columns",
                                    onclick: {
                                        let id = id.clone();
                                        let on_set_cols = on_set_cols.clone();
                                        move |_| {
                                            if let Some(handler) = &on_set_cols {
                                                handler.call((id.clone(), c));
                                            }
                                        }
                                    },
                                    span {
                                        class: "dash-wglyph",
                                        for i in 0..3usize {
                                            span {
                                                key: "wc{i}",
                                                class: if i < c { "dash-wcell on" } else { "dash-wcell" },
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if height_resizable {
                        span {
                            class: "dash-size-group",
                            span { class: "dash-col-label", "Height" }
                            div {
                                class: "seg dash-col-seg",
                                for r in 1..=3usize {
                                    button {
                                        key: "h{r}",
                                        class: if rows == r { "active" } else { "" },
                                        "aria-label": "Height level {r}",
                                        onclick: {
                                            let id = id.clone();
                                            let on_set_rows = on_set_rows.clone();
                                            move |_| {
                                                if let Some(handler) = &on_set_rows {
                                                    handler.call((id.clone(), r));
                                                }
                                            }
                                        },
                                        span {
                                            class: "dash-hglyph",
                                            for i in 0..3usize {
                                                span {
                                                    key: "hc{i}",
                                                    class: if i >= 3 - r { "dash-hcell on" } else { "dash-hcell" },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        span {
                            class: "dash-col-label dash-fixed-h",
                            title: "This widget sizes to its content",
                            "Fixed height"
                        }
                    }
                    button {
                        class: "btn-icon focus-ring dash-widget-remove",
                        title: "Remove",
                        onclick: {
                            let id = id.clone();
                            let on_remove = on_remove.clone();
                            move |_| {
                                if let Some(handler) = &on_remove {
                                    handler.call(id.clone());
                                }
                            }
                        },
                        Icon { name: IconName::X, size: 13 }
                    }
                }
            }

            // Header: icon + uppercase title + optional View action
            div {
                class: "dash-w-head",
                div {
                    style: "display:flex; align-items:center; gap:8px; min-width:0;",
                    span {
                        style: "color: var(--cf-text-muted); flex-shrink:0; display:inline-flex;",
                        Icon { name: icon, size: 13 }
                    }
                    h3 { class: "dash-w-title", "{title}" }
                }
                if let Some(label) = action_label.clone() {
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        onclick: move |_| {
                            if let Some(handler) = &on_action {
                                handler.call(());
                            }
                        },
                        "{label}"
                    }
                }
            }

            // Body
            div {
                class: "dash-w-body",
                {children}
            }
        }
    }
}

/// Container for the dashboard widget grid (3-column dense grid).
#[component]
pub fn WidgetGrid(children: Element) -> Element {
    rsx! {
        div {
            class: "dash-grid",
            "data-testid": "dashboard-grid",
            {children}
        }
    }
}

/// Stored layout for persistence: (id, cols, rows) in display order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct StoredLayout {
    pub version: u32,
    pub entries: Vec<(String, usize, usize)>,
}

impl StoredLayout {
    pub const VERSION: u32 = 2;
    const KEY: &'static str = "cf-dashboard-layout";

    /// Load from localStorage.
    pub fn load() -> Option<Self> {
        #[cfg(target_arch = "wasm32")]
        {
            let window = web_sys::window()?;
            let storage = window.local_storage().ok()??;
            let json = storage.get_item(Self::KEY).ok()??;
            serde_json::from_str(&json).ok()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            None
        }
    }

    /// Save to localStorage.
    pub fn save(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(json) = serde_json::to_string(self) {
                        let _ = storage.set_item(Self::KEY, &json);
                    }
                }
            }
        }
    }

    /// Remove the persisted layout (used by "Reset").
    pub fn clear() {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    let _ = storage.remove_item(Self::KEY);
                }
            }
        }
    }

    /// Whether a persisted layout exists.
    pub fn exists() -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            let Some(window) = web_sys::window() else {
                return false;
            };
            let Ok(Some(storage)) = window.local_storage() else {
                return false;
            };
            storage.get_item(Self::KEY).ok().flatten().is_some()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            false
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl serde::Serialize for StoredLayout {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("StoredLayout", 2)?;
        state.serialize_field("version", &self.version)?;
        state.serialize_field("entries", &self.entries)?;
        state.end()
    }
}

#[cfg(target_arch = "wasm32")]
impl<'de> serde::Deserialize<'de> for StoredLayout {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct StoredLayoutVisitor;

        impl<'de> Visitor<'de> for StoredLayoutVisitor {
            type Value = StoredLayout;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("stored dashboard layout")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut version = None;
                let mut entries = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "version" => version = Some(map.next_value::<u32>()?),
                        "entries" => {
                            entries = Some(map.next_value::<Vec<(String, usize, usize)>>()?)
                        }
                        _ => {
                            let _: de::IgnoredAny = map.next_value()?;
                        }
                    }
                }

                Ok(StoredLayout {
                    version: version.unwrap_or(StoredLayout::VERSION),
                    entries: entries.unwrap_or_default(),
                })
            }
        }

        deserializer.deserialize_map(StoredLayoutVisitor)
    }
}
