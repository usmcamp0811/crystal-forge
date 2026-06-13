//! Modal for deploying a system with commit selection.
//!
//! Matches the design DeployModal layout: flake select, branch select,
//! and a commit radio-button list.

use crate::api::models::{CommitInfo, DeploySystemRequest};
use dioxus::prelude::*;

/// Branch options for the deploy branch select.
const BRANCHES: &[&str] = &["main", "staging", "dev"];

#[component]
pub fn DeploySystemModal(
    system_id: String,
    hostname: String,
    deployment_policy: String,
    flake_name: String,
    flake_branch: String,
    flake_names: Vec<String>,
    commits: Vec<CommitInfo>,
    current_commit: Option<String>,
    #[props(default)] error_message: Option<String>,
    on_close: EventHandler<()>,
    on_deploy: EventHandler<DeploySystemRequest>,
) -> Element {
    let mut selected_commit = use_signal(|| current_commit.clone().unwrap_or_default());
    let mut selected_flake = use_signal(|| flake_name.clone());
    let mut selected_branch = use_signal(|| flake_branch.clone());
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
                        "Select a commit from "
                        span { class: "mono", "{flake_name}" }
                        " to deploy."
                    }
                }

                div {
                    class: "modal-body",
                    style: "overflow-y:auto;",

                    if is_auto_latest {
                        // Auto-latest notice
                        div {
                            class: "rounded-lg border border-blue-500/30 bg-blue-500/10 px-4 py-3 text-sm text-blue-200",
                            "This system is set to auto-latest. Manual deployment is not available."
                        }
                        if let Some(ref current) = current_commit {
                            div {
                                class: "mt-3",
                                label { class: "label", "Current Deployment" }
                                div { class: "mono text-xs", style: "color: var(--cf-text-primary);", "{current}" }
                            }
                        }
                    } else {
                        // Manual/Pinned deployment UI

                        // Flake select (design: first field in deploy modal)
                        div {
                            class: "field",
                            label { class: "label", "Flake" }
                            select {
                                class: "input focus-ring",
                                value: "{selected_flake}",
                                onchange: move |e| selected_flake.set(e.value().clone()),
                                for name in &flake_names {
                                    option {
                                        value: "{name}",
                                        selected: *selected_flake.read() == *name,
                                        "{name}"
                                    }
                                }
                            }
                        }

                        // Branch select (design: second field)
                        div {
                            class: "field",
                            label { class: "label", "Branch" }
                            select {
                                class: "input focus-ring",
                                value: "{selected_branch}",
                                onchange: move |e| selected_branch.set(e.value().clone()),
                                for b in BRANCHES {
                                    option {
                                        value: "{b}",
                                        selected: *selected_branch.read() == *b,
                                        "{b}"
                                    }
                                }
                            }
                        }

                        // Current commit reference
                        if let Some(ref current) = current_commit {
                            div {
                                class: "field",
                                label { class: "label", "Currently Deployed" }
                                div { class: "mono text-xs", style: "color: var(--cf-text-primary);", "{current}" }
                            }
                        }

                        // Commit selector (design: radio-button style list)
                        div {
                            class: "field",
                            label { class: "label", "Select Commit to Deploy" }

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
                                "Deploying…"
                            } else {
                                "Deploy commit"
                            }
                        }
                    }
                }
            }
        }
    }
}
