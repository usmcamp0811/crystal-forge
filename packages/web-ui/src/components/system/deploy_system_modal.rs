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
    #[props(default)] error_message: Option<String>,
    on_close: EventHandler<()>,
    on_deploy: EventHandler<DeploySystemRequest>,
) -> Element {
    let mut selected_commit = use_signal(|| current_commit.clone().unwrap_or_default());
    let mut is_deploying = use_signal(|| false);

    {
        let error_message = error_message.clone();
        use_effect(move || {
            if error_message.is_some() {
                is_deploying.set(false);
            }
        });
    }

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
        div {
            class: "modal-backdrop",
            onclick: move |_| on_close.call(()),

            div {
                class: "modal",
                style: "width:min(620px,96vw); max-height:92vh;",
                onclick: move |e| e.stop_propagation(),

                div {
                    class: "modal-head",
                    h2 {
                        "Deploy to {hostname}"
                    }
                    p {
                        "Select a commit to deploy for this system."
                    }
                }

                div {
                    class: "modal-body",
                    style: "overflow-y:auto;",

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
                                class: "sd-commit-list",
                                style: "max-height:220px;",

                                if commits.is_empty() {
                                    div {
                                        class: "empty",
                                        style: "margin: 12px;",
                                        p { "No commits available" }
                                    }
                                } else {
                                    for commit in &commits {
                                        button {
                                            key: "{commit.sha}",
                                            class: if selected_commit() == commit.sha { "sd-commit-item focus-ring selected" } else { "sd-commit-item focus-ring" },
                                            onclick: {
                                                let sha = commit.sha.clone();
                                                move |_| selected_commit.set(sha.clone())
                                            },

                                            div {
                                                class: "sd-commit-sha",
                                                "{commit.short_sha}"
                                            }
                                            div {
                                                class: "sd-commit-msg",
                                                "{commit.message}"
                                            }
                                            div {
                                                class: "sd-commit-meta mono",
                                                "{commit.author}"
                                            }
                                            div {
                                                class: "sd-commit-meta",
                                                "{commit.timestamp}"
                                            }
                                            if current_commit.as_ref() == Some(&commit.sha) {
                                                span { class: "chip chip-info", style: "font-size:10px", "current" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(message) = &error_message {
                        div {
                            class: "rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-200",
                            "{message}"
                        }
                    }
                }

                div {
                    class: "modal-foot",

                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| on_close.call(()),
                        disabled: is_deploying(),
                        "Cancel"
                    }

                    if can_deploy {
                        button {
                            class: "btn btn-primary focus-ring disabled:opacity-50 disabled:cursor-not-allowed",
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
