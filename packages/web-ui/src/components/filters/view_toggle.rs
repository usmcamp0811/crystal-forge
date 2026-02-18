//! View toggle component for switching between table and card views.

use dioxus::prelude::*;

use crate::theme;

/// View mode for list displays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    /// Show data in a table format
    Table,
    /// Show data as cards
    Cards,
}

impl ViewMode {
    /// Parse from storage string.
    pub fn from_storage(value: Option<String>) -> Self {
        match value.as_deref() {
            Some("cards") => Self::Cards,
            _ => Self::Table,
        }
    }

    /// Convert to storage string.
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Cards => "cards",
        }
    }
}

/// Toggle switch between table and card view modes.
///
/// # Example
/// ```ignore
/// let mut view_mode = use_signal(|| ViewMode::Table);
///
/// rsx! {
///     ViewToggle {
///         view_mode: *view_mode.read(),
///         on_change: move |mode| view_mode.set(mode)
///     }
/// }
/// ```
#[component]
pub fn ViewToggle(view_mode: ViewMode, on_change: EventHandler<ViewMode>) -> Element {
    let table_active = view_mode == ViewMode::Table;
    let cards_active = view_mode == ViewMode::Cards;

    rsx! {
        div {
            class: "inline-flex rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG}",
            button {
                class: "px-3 py-2 text-sm font-medium rounded-l-lg transition {theme::interactive::FOCUS_RING} {theme::text::SECONDARY} {active_class(table_active)}",
                onclick: move |_| on_change.call(ViewMode::Table),
                "Table"
            }
            button {
                class: "px-3 py-2 text-sm font-medium rounded-r-lg transition {theme::interactive::FOCUS_RING} {theme::text::SECONDARY} {active_class(cards_active)}",
                onclick: move |_| on_change.call(ViewMode::Cards),
                "Cards"
            }
        }
    }
}

/// Get the CSS class for active/inactive state.
fn active_class(is_active: bool) -> &'static str {
    if is_active {
        "bg-gray-700 text-white"
    } else {
        "hover:text-white"
    }
}
