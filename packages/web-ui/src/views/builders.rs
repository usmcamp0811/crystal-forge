//! Builders management view.

use dioxus::prelude::*;

use crate::components::builders::{BuilderMetricsView, BuildersList};
use crate::theme;

fn came_from_setup() -> bool {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let flag = storage.get_item("cf.from_setup").ok().flatten();
        if flag.as_deref() == Some("1") {
            let _ = storage.remove_item("cf.from_setup");
            return true;
        }
    }
    false
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
    let from_setup = use_signal(came_from_setup);

    rsx! {
        div {
            class: "space-y-6",

            // Setup coach guidance shown when navigated from coach steps.
            if from_setup() {
                div {
                    "data-testid": "setup-coach-builders-callout",
                    style: "background:rgba(109,40,217,0.2); border:1px solid rgba(139,92,246,0.5); border-radius:8px; padding:12px 16px;",
                    p { style: "color:#e9d5ff; font-size:12px; font-weight:700; margin:0; letter-spacing:0.03em; text-transform:uppercase;", "Setup Tour - Step 3 of 6" }
                    p { style: "color:#e9d5ff; font-size:14px; font-weight:600; margin:4px 0 0 0;", "Connect a builder" }
                    p { style: "color:#ddd6fe; font-size:13px; margin:4px 0 0 0;", "Use Add Builder to register a worker that evaluates and builds your flake changes." }
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
                    BuildersList {
                        show_onboarding_hint: from_setup(),
                    }
                },
                BuildersTab::Metrics => rsx! {
                    BuilderMetricsView {}
                },
            }
        }
    }
}
