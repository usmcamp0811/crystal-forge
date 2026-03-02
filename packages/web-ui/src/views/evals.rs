//! Evaluation queue view - shows commits being evaluated in real-time.

use dioxus::prelude::*;
use chrono::{DateTime, Utc};

use crate::api;
use crate::components::{Card, EvalLogModal};
use crate::theme;

/// Evaluation queue page - shows active and pending evaluations
#[component]
pub fn EvalsView() -> Element {
    let mut eval_log_modal_open = use_signal(|| None::<(i32, String)>);
    let refresh_trigger = use_signal(|| 0_u64);
    
    // Fetch evaluations (reusing dashboard for now until we add dedicated endpoint)
    let dashboard = use_resource(move || async move {
        let _ = refresh_trigger();
        api::client::fetch_dashboard().await
    });

    // Auto-refresh every 5 seconds
    use_effect(move || {
        let mut refresh = refresh_trigger.clone();
        spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                refresh.write().wrapping_add(1);
            }
        });
    });

    rsx! {
        div {
            class: "space-y-6",
            
            // Header
            div {
                class: "flex items-center justify-between",
                h1 {
                    class: "text-3xl font-bold {theme::text::PRIMARY}",
                    "Evaluation Queue"
                }
                div {
                    class: "flex items-center gap-2",
                    button {
                        class: "px-3 py-1.5 text-sm rounded {theme::button::SECONDARY}",
                        onclick: move |_| {
                            refresh_trigger.write().wrapping_add(1);
                        },
                        "🔄 Refresh"
                    }
                }
            }

            // Eval Queue Card
            Card {
                title: Some("Active & Pending Evaluations".to_string()),
                children: rsx! {
                    div {
                        class: "overflow-x-auto",
                        
                        match &*dashboard.read() {
                            Some(Ok(summary)) => {
                                // For now, show recent commits from timeline
                                // TODO: Add dedicated eval queue endpoint
                                rsx! {
                                    if let Some(timeline) = summary.timeline.first() {
                                        div {
                                            class: "min-w-full",
                                            EvalQueueTable {
                                                commits: timeline.commits.clone(),
                                                flake_name: timeline.flake_name.clone(),
                                                eval_log_modal_open: eval_log_modal_open,
                                            }
                                        }
                                    } else {
                                        p {
                                            class: "text-sm {theme::text::SECONDARY} p-4",
                                            "No evaluations in queue"
                                        }
                                    }
                                }
                            }
                            Some(Err(e)) => rsx! {
                                p {
                                    class: "text-sm text-red-400 p-4",
                                    "Error loading evaluations: {e}"
                                }
                            },
                            None => rsx! {
                                p {
                                    class: "text-sm {theme::text::SECONDARY} p-4",
                                    "Loading..."
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Eval Log Modal
        if let Some((commit_id, commit_hash)) = eval_log_modal_open.read().clone() {
            EvalLogModal {
                commit_id: commit_id,
                commit_hash: commit_hash,
                on_close: move |_| {
                    eval_log_modal_open.set(None);
                }
            }
        }
    }
}

#[component]
fn EvalQueueTable(
    commits: Vec<crate::api::models::FlakeCommit>,
    flake_name: String,
    eval_log_modal_open: Signal<Option<(i32, String)>>,
) -> Element {
    // Filter to only show commits that are being evaluated or pending
    let active_commits: Vec<_> = commits
        .iter()
        .filter(|c| {
            c.evaluation_status.as_deref() == Some("in_progress") 
            || c.evaluation_status.as_deref() == Some("pending")
        })
        .collect();
    
    if active_commits.is_empty() {
        return rsx! {
            p {
                class: "text-sm {theme::text::SECONDARY} p-4",
                "No active evaluations for {flake_name}"
            }
        };
    }

    rsx! {
        table {
            class: "min-w-full divide-y {theme::border::SECONDARY}",
            thead {
                tr {
                    class: "text-left",
                    th { class: "px-4 py-3 text-xs font-medium {theme::text::SECONDARY} uppercase tracking-wider", "Flake" }
                    th { class: "px-4 py-3 text-xs font-medium {theme::text::SECONDARY} uppercase tracking-wider", "Commit" }
                    th { class: "px-4 py-3 text-xs font-medium {theme::text::SECONDARY} uppercase tracking-wider", "Status" }
                    th { class: "px-4 py-3 text-xs font-medium {theme::text::SECONDARY} uppercase tracking-wider", "Systems" }
                    th { class: "px-4 py-3 text-xs font-medium {theme::text::SECONDARY} uppercase tracking-wider", "Actions" }
                }
            }
            tbody {
                class: "divide-y {theme::border::SECONDARY}",
                for commit in active_commits {
                    {
                        let commit_id = commit.id;
                        let commit_hash = commit.hash.clone();
                        let short_hash = &commit_hash[..7.min(commit_hash.len())];
                        let status = commit.evaluation_status.as_deref().unwrap_or("unknown");
                        let status_color = match status {
                            "in_progress" => "text-yellow-400",
                            "pending" => "text-gray-400",
                            "complete" => "text-green-400",
                            "failed" => "text-red-400",
                            _ => "text-gray-500"
                        };
                        
                        rsx! {
                            tr {
                                key: "{commit_id}",
                                class: "hover:bg-slate-800/50 transition-colors",
                                td {
                                    class: "px-4 py-3 text-sm {theme::text::PRIMARY} font-mono",
                                    "{flake_name}"
                                }
                                td {
                                    class: "px-4 py-3 text-sm font-mono {theme::text::SECONDARY}",
                                    "{short_hash}"
                                }
                                td {
                                    class: "px-4 py-3 text-sm",
                                    span {
                                        class: "px-2 py-1 rounded text-xs font-medium {status_color} bg-slate-800 border border-current",
                                        "{status}"
                                    }
                                }
                                td {
                                    class: "px-4 py-3 text-sm {theme::text::SECONDARY}",
                                    "{commit.systems.len()} systems"
                                }
                                td {
                                    class: "px-4 py-3 text-sm",
                                    button {
                                        class: "px-3 py-1 text-xs rounded {theme::button::PRIMARY}",
                                        onclick: move |_| {
                                            eval_log_modal_open.set(Some((commit_id, commit_hash.clone())));
                                        },
                                        "📋 View Logs"
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
