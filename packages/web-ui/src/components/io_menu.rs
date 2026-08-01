//! Shared Import / Export menu component (AC #35).
//!
//! A keyboard-accessible dropdown menu with ARIA roles used by the Compliance
//! and Policies views to surface import and export actions.
//!
//! Keyboard behaviour:
//! - `Enter` on trigger opens the menu.
//! - `Escape` closes the menu and returns focus to the trigger button (via DOM id).
//! - `Arrow Down` / `Arrow Up` moves roving focus within enabled items.
//! - `Home` / `End` jump to first / last enabled item.
//! - Clicking outside the menu closes it.

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use crate::components::icon::{Icon, IconName};

/// A single item in an [`IOMenu`].
#[derive(Clone, PartialEq)]
pub enum IOMenuItem {
    /// A clickable action.
    Action {
        label: String,
        icon: Option<IconName>,
        disabled: bool,
        disabled_reason: Option<String>,
        danger: bool,
    },
    /// A visual separator between groups of actions.
    Separator,
}

impl IOMenuItem {
    pub fn action(label: impl Into<String>) -> Self {
        IOMenuItem::Action {
            label: label.into(),
            icon: None,
            disabled: false,
            disabled_reason: None,
            danger: false,
        }
    }

    pub fn action_with_icon(label: impl Into<String>, icon: IconName) -> Self {
        IOMenuItem::Action {
            label: label.into(),
            icon: Some(icon),
            disabled: false,
            disabled_reason: None,
            danger: false,
        }
    }

    pub fn disabled(label: impl Into<String>, reason: impl Into<String>) -> Self {
        IOMenuItem::Action {
            label: label.into(),
            icon: None,
            disabled: true,
            disabled_reason: Some(reason.into()),
            danger: false,
        }
    }

    fn is_enabled_action(&self) -> bool {
        matches!(
            self,
            IOMenuItem::Action {
                disabled: false,
                ..
            }
        )
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct IOMenuProps {
    /// Menu items to display.
    pub items: Vec<IOMenuItem>,
    /// Called with the zero-based index of the clicked action.
    pub on_action: EventHandler<usize>,
    /// Label on the trigger button.
    #[props(default = "Import / Export".to_string())]
    pub trigger_label: String,
    /// Additional CSS classes for the trigger button.
    #[props(default)]
    pub trigger_class: String,
    /// Unique id suffix for DOM focus management (must be unique on the page).
    #[props(default = "io-menu".to_string())]
    pub id: String,
}

/// Focus an element by id using the web_sys DOM.
fn focus_by_id(id: &str) {
    if let Some(win) = web_sys::window() {
        if let Some(doc) = win.document() {
            if let Some(el) = doc.get_element_by_id(id) {
                let _ = el.dyn_ref::<web_sys::HtmlElement>().map(|h| h.focus().ok());
            }
        }
    }
}

/// A reusable accessible Import / Export dropdown menu.
///
/// Renders a trigger button and a dropdown panel. Closes on Escape and
/// outside-click, returning keyboard focus to the trigger in both cases.
#[component]
pub fn IOMenu(props: IOMenuProps) -> Element {
    let mut open = use_signal(|| false);
    // Roving focus within the menu: tracks the element-level index of the
    // currently focused item (across all items including separators).
    let mut focused_item: Signal<Option<usize>> = use_signal(|| None);

    // Stable ids for DOM focus.
    let trigger_id = format!("{}-trigger", props.id);

    // Indices of enabled (non-disabled, non-separator) items.
    let enabled_indices: Vec<usize> = props
        .items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            if item.is_enabled_action() {
                Some(i)
            } else {
                None
            }
        })
        .collect();

    let close_and_refocus = {
        let trigger_id = trigger_id.clone();
        move || {
            open.set(false);
            focused_item.set(None);
            focus_by_id(&trigger_id);
        }
    };

    let toggle = {
        let mut close_and_refocus = close_and_refocus.clone();
        let enabled_first = enabled_indices.first().copied();
        move |_| {
            if open() {
                close_and_refocus();
            } else {
                open.set(true);
                focused_item.set(enabled_first);
            }
        }
    };

    // Action-only index counter (separators don't consume a logical index).
    let mut action_idx: usize = 0;

    rsx! {
        div {
            class: "io-menu-container",
            style: "position:relative; display:inline-block;",

            // Backdrop closes the menu when clicking outside.
            if open() {
                div {
                    style: "position:fixed; inset:0; z-index:999;",
                    onclick: {
                        let mut close_and_refocus = close_and_refocus.clone();
                        move |_: Event<MouseData>| close_and_refocus()
                    },
                }
            }

            // Trigger button.
            button {
                id: trigger_id.clone(),
                class: format!("btn btn-ghost io-menu-trigger {}", props.trigger_class),
                style: "display:flex; align-items:center; gap:6px;",
                "aria-haspopup": "menu",
                "aria-expanded": if open() { "true" } else { "false" },
                onclick: toggle,
                onkeydown: {
                    let mut close_and_refocus = close_and_refocus.clone();
                    let enabled_first = enabled_indices.first().copied();
                    move |evt: Event<KeyboardData>| {
                        match evt.key() {
                            Key::Escape => {
                                close_and_refocus();
                            }
                            Key::ArrowDown => {
                                // Let arrow-down open the menu from the trigger.
                                // Do NOT handle Enter here: the native button click fires after
                                // keydown, which would immediately toggle the menu closed again.
                                evt.prevent_default();
                                if !open() {
                                    open.set(true);
                                    focused_item.set(enabled_first);
                                }
                            }
                            _ => {}
                        }
                    }
                },
                Icon { name: IconName::Download, size: 14 }
                "{props.trigger_label}"
                Icon { name: IconName::ChevronDown, size: 12 }
            }

            // Dropdown panel.
            if open() {
                div {
                    role: "menu",
                    class: "io-menu-panel",
                    style: "position:absolute; right:0; top:calc(100% + 4px); z-index:1000; \
                            min-width:220px; background:var(--card-bg, #1e2030); \
                            border:1px solid var(--border, rgba(255,255,255,.1)); \
                            border-radius:8px; padding:4px 0; box-shadow:0 8px 24px rgba(0,0,0,.4);",
                    onkeydown: {
                        let enabled_indices = enabled_indices.clone();
                        let mut close_and_refocus = close_and_refocus.clone();
                        let menu_id = props.id.clone();
                        move |evt: Event<KeyboardData>| {
                            match evt.key() {
                                Key::Escape => close_and_refocus(),
                                Key::ArrowDown => {
                                    evt.prevent_default(); // stop page scroll
                                    if let Some(cur) = focused_item() {
                                        let next = enabled_indices
                                            .iter()
                                            .position(|&i| i == cur)
                                            .and_then(|pos| enabled_indices.get(pos + 1))
                                            .copied()
                                            .unwrap_or(cur);
                                        focused_item.set(Some(next));
                                        focus_by_id(&format!("{}-item-{}", menu_id, next));
                                    }
                                }
                                Key::ArrowUp => {
                                    evt.prevent_default(); // stop page scroll
                                    if let Some(cur) = focused_item() {
                                        let prev = enabled_indices
                                            .iter()
                                            .position(|&i| i == cur)
                                            .and_then(|pos| pos.checked_sub(1))
                                            .and_then(|p| enabled_indices.get(p))
                                            .copied()
                                            .unwrap_or(cur);
                                        focused_item.set(Some(prev));
                                        focus_by_id(&format!("{}-item-{}", menu_id, prev));
                                    }
                                }
                                Key::Home => {
                                    evt.prevent_default();
                                    if let Some(&first) = enabled_indices.first() {
                                        focused_item.set(Some(first));
                                        focus_by_id(&format!("{}-item-{}", menu_id, first));
                                    }
                                }
                                Key::End => {
                                    evt.prevent_default();
                                    if let Some(&last) = enabled_indices.last() {
                                        focused_item.set(Some(last));
                                        focus_by_id(&format!("{}-item-{}", menu_id, last));
                                    }
                                }
                                _ => {}
                            }
                        }
                    },

                    for (item_idx, item) in props.items.iter().enumerate() {
                        match item {
                            IOMenuItem::Separator => rsx! {
                                hr {
                                    style: "border:none; border-top:1px solid var(--border, rgba(255,255,255,.1)); margin:4px 0;",
                                }
                            },
                            IOMenuItem::Action { label, icon, disabled, disabled_reason, danger } => {
                                let current_action_idx = action_idx;
                                action_idx += 1;
                                let on_action = props.on_action.clone();
                                let is_disabled = *disabled;
                                let is_danger = *danger;
                                let label = label.clone();
                                let icon = icon.clone();
                                let reason = disabled_reason.clone().unwrap_or_default();
                                let item_id = format!("{}-item-{}", props.id, item_idx);
                                let is_initially_focused =
                                    focused_item().map_or(false, |fi| fi == item_idx);
                                // Need separate owned clones for each event handler.
                                let mut close_click = close_and_refocus.clone();
                                let mut close_key = close_and_refocus.clone();
                                rsx! {
                                    button {
                                        id: item_id.clone(),
                                        role: "menuitem",
                                        class: format!(
                                            "io-menu-item {}",
                                            if is_danger { "io-menu-item--danger" } else { "" }
                                        ),
                                        style: format!(
                                            "display:flex; align-items:center; gap:8px; width:100%; \
                                             padding:8px 14px; background:none; border:none; \
                                             text-align:left; cursor:{}; font-size:13px; \
                                             color:{}; opacity:{};",
                                            if is_disabled { "not-allowed" } else { "pointer" },
                                            if is_danger { "var(--red-400, #f87171)" }
                                            else { "var(--text-primary, #e2e8f0)" },
                                            if is_disabled { "0.45" } else { "1.0" }
                                        ),
                                        disabled: is_disabled,
                                        title: if is_disabled { reason.as_str() } else { "" },
                                        // Roving tabindex: the initially focused item gets 0.
                                        tabindex: if is_initially_focused && !is_disabled { "0" } else { "-1" },
                                        // Auto-focus the first enabled item when the menu opens.
                                        onmounted: {
                                            let item_id_clone = item_id.clone();
                                            move |_| {
                                                if is_initially_focused && !is_disabled {
                                                    focus_by_id(&item_id_clone);
                                                }
                                            }
                                        },
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            if !is_disabled {
                                                on_action.call(current_action_idx);
                                                close_click();
                                            }
                                        },
                                        onkeydown: move |evt: Event<KeyboardData>| {
                                            if evt.key() == Key::Escape {
                                                close_key();
                                            }
                                        },
                                        if let Some(ic) = &icon {
                                            Icon { name: ic.clone(), size: 14 }
                                        }
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
