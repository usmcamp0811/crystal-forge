//! Systems list view — table of all registered NixOS systems.

use dioxus::prelude::*;

use crate::views::systems_list::SystemsListView;

/// The systems list page.
#[component]
pub fn SystemsView() -> Element {
    rsx! {
        SystemsListView {}
    }
}
