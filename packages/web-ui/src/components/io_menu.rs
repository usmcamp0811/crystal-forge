//! Shared Import / Export menu component (AC #35).
//!
//! A keyboard-accessible dropdown menu with ARIA roles used by the Compliance
//! and Policies views to surface import and export actions.

use dioxus::prelude::*;

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
}

#[derive(Props, Clone, PartialEq)]
pub struct IOMenuProps {
    /// Menu items to display.
    pub items: Vec<IOMenuItem>,
    /// Called with the zero-based index of the clicked action (separators are
    /// skipped in the index count; only `Action` items are indexed).
    pub on_action: EventHandler<usize>,
    /// Label on the trigger button.
    #[props(default = "Import / Export".to_string())]
    pub trigger_label: String,
    /// Additional CSS classes for the trigger button.
    #[props(default)]
    pub trigger_class: String,
}

/// A reusable accessible Import / Export dropdown menu.
///
/// Renders a trigger button and a dropdown panel. Closes on Escape and
/// outside-click. Focus returns to the trigger on close.
#[component]
pub fn IOMenu(props: IOMenuProps) -> Element {
    let mut open = use_signal(|| false);

    let toggle = move |_| {
        open.set(!open());
    };

    let close = move |_: Event<MouseData>| {
        open.set(false);
    };

    // Action-only index (separators don't consume an index)
    let mut action_idx: usize = 0;

    rsx! {
        div {
            class: "io-menu-container",
            style: "position:relative; display:inline-block;",

            // Backdrop to catch outside-clicks
            if open() {
                div {
                    style: "position:fixed; inset:0; z-index:999;",
                    onclick: close,
                }
            }

            // Trigger button
            button {
                class: format!("btn btn-ghost io-menu-trigger {}", props.trigger_class),
                style: "display:flex; align-items:center; gap:6px;",
                "aria-haspopup": "menu",
                "aria-expanded": if open() { "true" } else { "false" },
                onclick: toggle,
                onkeydown: move |evt: Event<KeyboardData>| {
                    if evt.key() == Key::Escape {
                        open.set(false);
                    }
                },
                Icon { name: IconName::Download, size: 14 }
                "{props.trigger_label}"
                Icon { name: IconName::ChevronDown, size: 12 }
            }

            // Dropdown panel
            if open() {
                div {
                    role: "menu",
                    class: "io-menu-panel",
                    style: "position:absolute; right:0; top:calc(100% + 4px); z-index:1000; \
                            min-width:220px; background:var(--card-bg, #1e2030); \
                            border:1px solid var(--border, rgba(255,255,255,.1)); \
                            border-radius:8px; padding:4px 0; box-shadow:0 8px 24px rgba(0,0,0,.4);",

                    for item in &props.items {
                        match item {
                            IOMenuItem::Separator => rsx! {
                                hr {
                                    style: "border:none; border-top:1px solid var(--border, rgba(255,255,255,.1)); margin:4px 0;",
                                }
                            },
                            IOMenuItem::Action { label, icon, disabled, disabled_reason, danger } => {
                                let current_idx = action_idx;
                                action_idx += 1;
                                let on_action = props.on_action.clone();
                                let is_disabled = *disabled;
                                let is_danger = *danger;
                                let label = label.clone();
                                let icon = icon.clone();
                                let reason = disabled_reason.clone().unwrap_or_default();
                                rsx! {
                                    button {
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
                                            if is_danger { "var(--red-400, #f87171)" } else { "var(--text-primary, #e2e8f0)" },
                                            if is_disabled { "0.45" } else { "1.0" }
                                        ),
                                        disabled: is_disabled,
                                        title: if is_disabled { reason.as_str() } else { "" },
                                        tabindex: if is_disabled { "-1" } else { "0" },
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            if !is_disabled {
                                                on_action.call(current_idx);
                                                open.set(false);
                                            }
                                        },
                                        onkeydown: move |evt: Event<KeyboardData>| {
                                            if evt.key() == Key::Escape {
                                                open.set(false);
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
