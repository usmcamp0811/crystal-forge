//! Builders management view.

use dioxus::prelude::*;

use crate::components::builders::{BuilderMetricsView, BuildersList};
use crate::theme;

fn came_from_setup() -> bool {
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .map(|q| q.contains("from=setup"))
        .unwrap_or(false)
}

#[derive(Clone, Copy, PartialEq)]
enum BuildersTab {
    List,
    Metrics,
}

/// Builders management page.
#[component]
pub fn BuildersView() -> Element {
    let mut active_tab = use_signal(|| BuildersTab::List);
    let from_setup = came_from_setup();

    rsx! {
        div {
            class: "space-y-6",

            // Back-to-wizard banner shown when navigated from the setup wizard
            if from_setup {
                div {
                    style: "background:rgba(109,40,217,0.2); border:1px solid rgba(139,92,246,0.5); border-radius:8px; padding:10px 16px; display:flex; align-items:center; justify-content:space-between; gap:12px;",
                    span { style: "color:#e9d5ff; font-size:14px;", "← You came here from the Setup Wizard" }
                    a {
                        href: "/setup",
                        style: "color:#a78bfa; font-size:13px; font-weight:500; white-space:nowrap; text-decoration:underline;",
                        "Back to Setup Wizard"
                    }
                }
            }

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
