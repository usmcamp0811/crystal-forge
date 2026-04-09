//! Modal for deploying a system with commit selection.

use crate::api::models::{CommitInfo, DeploySystemRequest};
use crate::theme;
use dioxus::prelude::*;

#[component]
pub fn DeploySystemModal(
    system_id: String,
    hostname: String,
    deployment_policy: String,
    commits: Vec<CommitInfo>,
    current_commit: Option<String>,
    on_close: EventHandler<()>,
    on_deploy: EventHandler<DeploySystemRequest>,
) -> Element {
    let mut selected_commit = use_signal(|| current_commit.clone().unwrap_or_default());
    let mut is_deploying = use_signal(|| false);

    let is_auto_latest = deployment_policy == "auto_latest";
    let can_deploy = matches!(deployment_policy.as_str(), "manual" | "pinned");

    let handle_deploy = move |_| {
        if !can_deploy || selected_commit.read().is_empty() {
            return;
        }

        is_deploying.set(true);

        let request = DeploySystemRequest {
            commit_sha: selected_commit.read().clone(),
        };

        on_deploy.call(request);
    };

    rsx! {
        // Modal backdrop
        div {
            class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
            onclick: move |_| on_close.call(()),

            // Modal content
            div {
                class: "bg-gray-900 rounded-xl border {theme::surface::CARD_BORDER} shadow-2xl w-full max-w-3xl mx-4 max-h-[90vh] overflow-hidden flex flex-col",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "px-6 py-4 border-b {theme::surface::CARD_BORDER}",
                    h2 {
                        class: "text-xl font-semibold text-white",
                        "Deploy System"
                    }
                    p {
                        class: "text-sm text-gray-400 mt-1",
                        "Deploy {hostname} to a specific commit"
                    }
                }

                // Content
                div {
                    class: "px-6 py-4 space-y-4 overflow-y-auto flex-1",

                    if is_auto_latest {
                        // Auto-latest notice
                        div {
                            class: "bg-blue-500/10 border border-blue-500/30 rounded-lg p-4",
                            div {
                                class: "flex items-start gap-3",
                                div {
                                    class: "text-blue-400",
                                    // Info icon
                                    svg {
                                        class: "w-5 h-5",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "none",
                                        view_box: "0 0 24 24",
                                        stroke: "currentColor",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            stroke_width: "2",
                                            d: "M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                                        }
                                    }
                                }
                                div {
                                    class: "flex-1",
                                    h3 { class: "text-sm font-semibold text-blue-300", "Auto-Deploy Enabled" }
                                    p { class: "text-sm text-gray-300 mt-1",
                                        "This system is configured for automatic deployments. Manual deployment is not available."
                                    }
                                }
                            }
                        }

                        // Show current commit (read-only)
                        if let Some(ref current) = current_commit {
                            div {
                                class: "mt-4",
                                label {
                                    class: "block text-sm font-medium text-gray-300 mb-2",
                                    "Current Deployment"
                                }
                                div {
                                    class: "bg-gray-800/50 border {theme::surface::CARD_BORDER} rounded-lg px-4 py-3",
                                    p { class: "font-mono text-sm text-emerald-400", "{current}" }
                                }
                            }
                        }
                    } else {
                        // Manual/Pinned deployment UI

                        // Current commit display
                        if let Some(ref current) = current_commit {
                            div {
                                label {
                                    class: "block text-sm font-medium text-gray-300 mb-2",
                                    "Currently Deployed"
                                }
                                div {
                                    class: "bg-gray-800/50 border {theme::surface::CARD_BORDER} rounded-lg px-4 py-3",
                                    p { class: "font-mono text-sm text-emerald-400", "{current}" }
                                }
                            }
                        }

                        // Commit selector
                        div {
                            label {
                                class: "block text-sm font-medium text-gray-300 mb-2",
                                "Select Commit to Deploy"
                            }

                            div {
                                class: "space-y-2 max-h-96 overflow-y-auto",

                                if commits.is_empty() {
                                    div {
                                        class: "text-center py-8 text-gray-500",
                                        p { "No commits available" }
                                    }
                                } else {
                                    for commit in &commits {
                                        div {
                                            key: "{commit.sha}",
                                            class: "border {theme::surface::CARD_BORDER} rounded-lg hover:bg-gray-800/50 transition-colors cursor-pointer",
                                            class: if selected_commit() == commit.sha { "bg-emerald-500/10 border-emerald-500/50" } else { "" },
                                            onclick: {
                                                let sha = commit.sha.clone();
                                                move |_| selected_commit.set(sha.clone())
                                            },

                                            div {
                                                class: "px-4 py-3",
                                                div {
                                                    class: "flex items-start justify-between gap-4",
                                                    div {
                                                        class: "flex-1 min-w-0",
                                                        p {
                                                            class: "text-sm font-medium text-white truncate",
                                                            "{commit.message}"
                                                        }
                                                        div {
                                                            class: "flex items-center gap-3 mt-1 text-xs text-gray-400",
                                                            span {
                                                                class: "font-mono",
                                                                "{commit.short_sha}"
                                                            }
                                                            span { "•" }
                                                            span { "{commit.author}" }
                                                            span { "•" }
                                                            span { "{commit.timestamp}" }
                                                        }
                                                    }
                                                    if selected_commit() == commit.sha {
                                                        div {
                                                            class: "text-emerald-400",
                                                            // Checkmark icon
                                                            svg {
                                                                class: "w-5 h-5",
                                                                xmlns: "http://www.w3.org/2000/svg",
                                                                fill: "none",
                                                                view_box: "0 0 24 24",
                                                                stroke: "currentColor",
                                                                path {
                                                                    stroke_linecap: "round",
                                                                    stroke_linejoin: "round",
                                                                    stroke_width: "2",
                                                                    d: "M5 13l4 4L19 7"
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
                }

                // Footer
                div {
                    class: "px-6 py-4 border-t {theme::surface::CARD_BORDER} flex justify-end gap-3",

                    button {
                        class: "px-4 py-2 text-gray-300 hover:text-white transition-colors",
                        onclick: move |_| on_close.call(()),
                        disabled: is_deploying(),
                        "Cancel"
                    }

                    if can_deploy {
                        button {
                            class: "px-4 py-2 bg-emerald-600 hover:bg-emerald-700 text-white rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                            onclick: handle_deploy,
                            disabled: is_deploying() || selected_commit.read().is_empty(),

                            if is_deploying() {
                                "Deploying..."
                            } else {
                                "Deploy"
                            }
                        }
                    }
                }
            }
        }
    }
}
