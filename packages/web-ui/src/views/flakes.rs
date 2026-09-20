//! Flakes list view — table of configured flakes.

use dioxus::prelude::*;

use crate::views::flakes_list::FlakesListViewNew;

/// Renders the flakes list while preserving URL-backed tray state.
#[component]
pub fn FlakesView(query: String) -> Element {
    rsx! {
        FlakesListViewNew { initial_query: query }
    }
}
