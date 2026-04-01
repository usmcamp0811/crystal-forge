//! Build queue pane component for the builds control center.

use dioxus::prelude::*;

use crate::components::layout::Card;
use crate::theme;

use super::helpers::{
    build_status_badge_class, extract_system_name, queue_row_style, short_commit, BuildAction,
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

    let filtered: Vec<BuildItem> = builds
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
                        "data-testid": "build-queue-list",
                        for build in filtered {
                            button {
                                key: "{build.id}",
                                class: "w-full rounded-xl border px-4 py-4 text-left transition min-h-[44px] {queue_row_style(*selected_id.read() == Some(build.id), build.status)}",
                                "data-testid": "build-queue-card",
                                onclick: move |_| selected_id.set(Some(build.id)),
                                div {
                                    class: "flex items-start justify-between gap-4 mb-3",
                                    div {
                                        class: "flex-1 min-w-0",
                                        div {
                                            class: "flex items-center gap-2 flex-wrap",
                                            p {
                                                class: "text-sm {theme::text::PRIMARY} font-semibold truncate",
                                                title: "{extract_system_name(&build.hostname)}",
                                                "{extract_system_name(&build.hostname)}"
                                            }
                                            span {
                                                class: "inline-flex px-2 py-0.5 text-[10px] rounded border cf-chip-blue shrink-0",
                                                "{build.flake}"
                                            }
                                        }
                                        p {
                                            class: "text-xs {theme::text::MUTED} mt-1 truncate",
                                            title: "{build.branch} · {short_commit(&build.commit)}",
                                            "{build.branch} · {short_commit(&build.commit)}"
                                        }
                                    }
                                    div {
                                        class: "text-right shrink-0",
                                        span {
                                            class: "inline-flex px-2 py-1 text-[10px] uppercase rounded border {build_status_badge_class(build.status)}",
                                            "{build.status_label()}"
                                        }
                                        p { class: "text-[10px] {theme::text::DISABLED} mt-1 whitespace-nowrap", "{build.queued_for}" }
                                    }
                                }

                                div {
                                    class: "mt-2 rounded-md border {theme::surface::CARD_BORDER} cf-subtle-bg px-3 py-2",
                                    p {
                                        class: "text-[11px] {theme::text::SECONDARY} leading-5 truncate",
                                        title: "Build target: {build.flake} · {extract_system_name(&build.hostname)}",
                                        span { class: "{theme::text::MUTED}", "Build target: " }
                                        span { class: "font-mono text-cyan-300", "{build.flake}" }
                                        span { class: "{theme::text::DISABLED} mx-1", "·" }
                                        span { class: "font-mono {theme::text::SECONDARY}", "{extract_system_name(&build.hostname)}" }
                                    }
                                    if !build.summary.is_empty() && build.summary != format!("job {}", build.job_id.map(|id| id.to_string()).unwrap_or_else(|| "unknown".to_string())) {
                                        p {
                                            class: "text-[11px] {theme::text::MUTED} mt-1 italic truncate",
                                            title: "{build.summary}",
                                            "{build.summary}"
                                        }
                                    }
                                }

                                div {
                                    class: "mt-3 flex flex-wrap items-center justify-between gap-3",
                                    div {
                                        class: "inline-flex items-center gap-2 text-[10px] flex-wrap",
                                        span {
                                            class: "inline-flex px-2 py-1 rounded border cf-chip-slate",
                                            "worker {build.worker_id}"
                                        }
                                        if let Some(runtime) = build.runtime {
                                            span {
                                                class: "inline-flex px-2 py-1 rounded border cf-chip-teal",
                                                "runtime {runtime}"
                                            }
                                        }
                                    }
                                    div {
                                        class: "inline-flex items-center gap-2 flex-wrap",
                                        if matches!(build.status, BuildStatus::Building | BuildStatus::Restarting) {
                                            button {
                                                class: "text-xs text-red-400 hover:text-red-300 px-3 py-1.5 rounded hover:bg-red-500/10 transition-colors min-h-[44px]",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    on_build_action.call((build.id, BuildAction::Stop));
                                                },
                                                "Stop"
                                            }
                                        }
                                        button {
                                            class: "text-xs px-3 py-1.5 rounded transition-colors cf-action-link min-h-[44px]",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                on_build_action.call((build.id, BuildAction::Restart));
                                            },
                                            "Restart"
                                        }
                                        if build.status == BuildStatus::Queued {
                                            button {
                                                class: "text-xs text-cyan-300 hover:text-cyan-200 px-3 py-1.5 rounded hover:bg-cyan-500/10 transition-colors min-h-[44px]",
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
