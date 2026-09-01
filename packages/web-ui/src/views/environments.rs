//! Environments view page.

use dioxus::prelude::*;

use crate::views::environments_list::EnvironmentsListView;

#[component]
pub fn EnvironmentsView(query: String) -> Element {
    rsx! {
        EnvironmentsListView { initial_query: query }
    }
}
