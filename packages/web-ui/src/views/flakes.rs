//! Flakes list view — table of configured flakes.

use dioxus::prelude::*;

use crate::views::flakes_list::FlakesListView;

/// The flakes list page.
#[component]
pub fn FlakesView() -> Element {
    rsx! {
        FlakesListView {}
    }
}
