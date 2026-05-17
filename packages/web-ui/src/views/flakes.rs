//! Flakes list view — table of configured flakes.

use dioxus::prelude::*;

use crate::views::flakes_list::FlakesListViewNew;

/// The flakes list page - now using the rebuilt pixel-perfect design.
#[component]
pub fn FlakesView() -> Element {
    rsx! {
        FlakesListViewNew {}
    }
}
