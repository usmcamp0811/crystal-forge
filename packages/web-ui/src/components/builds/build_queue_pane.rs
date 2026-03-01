//! Build queue pane component for the builds control center.

use dioxus::prelude::*;

use crate::components::layout::Card;
use crate::theme;

use super::helpers::{
    build_status_badge_style, queue_row_style, queue_sort_rank, short_commit, BuildAction,
    BuildItem, BuildStatus,
};

/// Build queue pane showing all queued and active builds.
#[component]
pub fn BuildQueuePane(
    builds: Vec<BuildItem>,
    selected_id: Signal<Option<i32>>,
    on_build_action: EventHandler<(i32, BuildAction)>,
) -> Element {
    let mut search = use_signal(String::new);

    let mut filtered: Vec<BuildItem> = builds
        .into_iter()
        .filter(|b| {
            let q = search.read().trim().to_lowercase();
            if q.is_empty() {
                true
            } else {
                b.hostname.to_lowercase().contains(&q)
                    || b.flake.to_lowercase().contains(&q)
                    || b.commit.to_lowercase().contains(&q)
            }
        })
        .collect();

    filtered.sort_by_key(|b| queue_sort_rank(b.status));

    rsx! {
        Card {
            title: Some("Queue".to_string()),
            children: rsx! {
                div {
                    class: "space-y-3",
                    input {
                        class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                        r#type: "search",
                        placeholder: "Search by host, flake, or commit...",
                        value: "{search.read()}",
                        oninput: move |evt| search.set(evt.value()),
                    }

                    div {
                        class: "space-y-2 max-h-[56vh] overflow-y-auto pr-1",
                        for build in filtered {
                            button {
                                key: "{build.id}",
                                class: "w-full rounded-xl border px-4 py-3 text-left transition",
                                style: "{queue_row_style(*selected_id.read() == Some(build.id), build.status)}",
                                onclick: move |_| selected_id.set(Some(build.id)),
                                div {
                                    class: "flex items-start justify-between gap-3",
                                    div {
                                        div {
                                            class: "flex items-center gap-2",
                                            p { class: "text-sm text-white font-semibold", "{build.hostname}" }
                                            span {
                                                class: "inline-flex px-2 py-0.5 text-[10px] rounded border text-blue-100",
                                                style: "background-color: #253449; border-color: #3E5B82;",
                                                "{build.flake}"
                                            }
                                        }
                                        p { class: "text-xs text-gray-300 mt-1", "{build.branch} · {short_commit(&build.commit)}" }
                                    }
                                    div {
                                        class: "text-right",
                                        span {
                                            class: "inline-flex px-2 py-1 text-[10px] uppercase rounded border",
                                            style: "{build_status_badge_style(build.status)}",
                                            "{build.status_label()}"
                                        }
                                        p { class: "text-[10px] text-gray-400 mt-1", "{build.queued_for}" }
                                    }
                                }

                                div {
                                    class: "mt-2 rounded-md border border-gray-700/60 bg-gray-950/70 px-2 py-1",
                                    p { class: "text-[11px] text-gray-300 font-mono leading-5", "{build.summary}" }
                                }

                                div {
                                    class: "mt-3 flex flex-wrap items-center justify-between gap-2",
                                    div {
                                        class: "inline-flex items-center gap-2 text-[10px]",
                                        span {
                                            class: "inline-flex px-2 py-1 rounded border text-gray-100",
                                            style: "background-color: #2B303B; border-color: #495264;",
                                            "worker {build.worker_id}"
                                        }
                                        if let Some(runtime) = build.runtime {
                                            span {
                                                class: "inline-flex px-2 py-1 rounded border text-gray-100",
                                                style: "background-color: #23363A; border-color: #3D6870;",
                                                "runtime {runtime}"
                                            }
                                        }
                                    }
                                    div {
                                        class: "inline-flex items-center gap-2",
                                        if matches!(build.status, BuildStatus::Building | BuildStatus::Restarting) {
                                            button {
                                                class: "text-xs text-red-400 hover:text-red-300 px-2 py-1 rounded hover:bg-red-500/10 transition-colors",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    on_build_action.call((build.id, BuildAction::Stop));
                                                },
                                                "Stop"
                                            }
                                        }
                                        button {
                                            class: "text-xs px-2 py-1 rounded transition-colors",
                                            style: "color: #D6C3E8;",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                on_build_action.call((build.id, BuildAction::Restart));
                                            },
                                            "Restart"
                                        }
                                        if build.status == BuildStatus::Queued {
                                            button {
                                                class: "text-xs text-cyan-300 hover:text-cyan-200 px-2 py-1 rounded hover:bg-cyan-500/10 transition-colors",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    on_build_action.call((build.id, BuildAction::RunNext));
                                                },
                                                "Run Next"
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
    }
}
