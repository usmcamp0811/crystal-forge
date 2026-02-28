//! Builders management view.

use dioxus::prelude::*;

use crate::components::builders::{BuilderMetricsView, BuildersList};
use crate::theme;

#[derive(Clone, Copy, PartialEq)]
enum BuildersTab {
    List,
    Metrics,
}

/// Builders management page.
#[component]
pub fn BuildersView() -> Element {
    let mut active_tab = use_signal(|| BuildersTab::List);

    rsx! {
        div {
            class: "space-y-6",

            header {
                class: "flex flex-col gap-4",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Builders" }
                    p {
                        class: "text-sm {theme::text::SECONDARY}",
                        "Manage build workers and monitor resource usage."
                    }
                }

                // Tabs
                div {
                    class: "flex border-b border-slate-700",
                    button {
                        class: if active_tab() == BuildersTab::List {
                            "px-4 py-2 border-b-2 border-blue-500 text-blue-400 font-medium"
                        } else {
                            "px-4 py-2 border-b-2 border-transparent text-slate-400 hover:text-white transition-colors"
                        },
                        onclick: move |_| active_tab.set(BuildersTab::List),
                        "Builders"
                    }
                    button {
                        class: if active_tab() == BuildersTab::Metrics {
                            "px-4 py-2 border-b-2 border-blue-500 text-blue-400 font-medium"
                        } else {
                            "px-4 py-2 border-b-2 border-transparent text-slate-400 hover:text-white transition-colors"
                        },
                        onclick: move |_| active_tab.set(BuildersTab::Metrics),
                        "Metrics"
                    }
                }
            }

            // Tab content
            match active_tab() {
                BuildersTab::List => rsx! {
                    BuildersList {}
                },
                BuildersTab::Metrics => rsx! {
                    BuilderMetricsView {}
                },
            }
        }
    }
}
