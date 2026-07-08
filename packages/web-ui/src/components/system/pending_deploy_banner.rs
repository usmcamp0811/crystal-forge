use crate::api::models::SystemDeploymentProgress;
use crate::components::{Icon, IconName};
use dioxus::prelude::*;

const DEPLOY_STAGES: [&str; 4] = ["queued", "picked_up", "applying", "activated"];

fn stage_index(stage: &str) -> usize {
    DEPLOY_STAGES
        .iter()
        .position(|candidate| *candidate == stage)
        .unwrap_or(0)
}

fn stage_label(stage: &str, is_rollback: bool) -> &'static str {
    match stage {
        "queued" => "Queued",
        "picked_up" => "Picked up",
        "applying" if is_rollback => "Reverting",
        "applying" => "Applying",
        "activated" => "Activated",
        "failed" => "Failed",
        _ => "Queued",
    }
}

fn stage_subtext(
    stage: &str,
    is_rollback: bool,
    hostname: &str,
    heartbeat_interval_secs: i64,
    target_label: &str,
) -> String {
    match stage {
        "queued" => format!(
            "Waiting for {hostname} agent to check in (heartbeat every {heartbeat_interval_secs}s)"
        ),
        "picked_up" if is_rollback => "Agent fetched the rollback command".to_string(),
        "picked_up" => "Agent fetched the deployment command".to_string(),
        "applying" if is_rollback => format!("Switching to {target_label}"),
        "applying" => format!("Building & switching to {target_label}"),
        "activated" => "Target configuration is live".to_string(),
        "failed" => "Deployment failed to activate. Review logs for details.".to_string(),
        _ => String::new(),
    }
}

#[component]
pub fn PendingDeployBanner(
    progress: SystemDeploymentProgress,
    hostname: String,
    heartbeat_interval_secs: i64,
    on_dismiss: EventHandler<()>,
    on_view_logs: EventHandler<()>,
) -> Element {
    let is_rollback = progress.kind == "rollback";
    let is_done = progress.stage == "activated";
    let is_failed = progress.stage == "failed";
    let verb = if is_rollback {
        "Rollback"
    } else {
        "Deployment"
    };
    let target_label = progress
        .target_generation
        .map(|generation| format!("gen #{generation}"))
        .or_else(|| progress.target_commit.clone())
        .unwrap_or_else(|| progress.target_store_path.clone());
    let target_chip = if is_rollback {
        progress
            .target_generation
            .map(|generation| format!("#{generation} · {target_label}"))
            .unwrap_or_else(|| target_label.clone())
    } else {
        target_label.clone()
    };
    let current_stage = if is_done {
        "activated"
    } else {
        progress.stage.as_str()
    };
    let current_idx = stage_index(current_stage);
    let subtext = stage_subtext(
        progress.stage.as_str(),
        is_rollback,
        hostname.as_str(),
        heartbeat_interval_secs,
        target_label.as_str(),
    );
    let root_class = format!(
        "deploy-pending{}{}{}",
        if is_done { " done" } else { "" },
        if is_rollback { " rollback" } else { "" },
        if is_failed { " failed" } else { "" }
    );

    rsx! {
        div { class: "{root_class}",
            div { class: "deploy-pending-main",
                div { class: "deploy-pending-icon",
                    if is_done {
                        Icon { name: IconName::Check, size: 16 }
                    } else if is_failed {
                        Icon { name: IconName::Warn, size: 16 }
                    } else {
                        span { class: "deploy-pending-spinner", "aria-hidden": "true" }
                    }
                }
                div { style: "min-width:0;flex:1;",
                    div { class: "deploy-pending-title",
                        if is_done {
                            "{verb} complete"
                        } else if is_failed {
                            "{verb} failed"
                        } else {
                            "{verb} in progress"
                        }
                        span { class: "mono deploy-pending-commit", "{target_chip}" }
                    }
                    div { class: "deploy-pending-sub", "{subtext}" }
                }
                button {
                    class: "btn btn-ghost xs focus-ring",
                    onclick: move |_| on_view_logs.call(()),
                    Icon { name: IconName::Terminal, size: 12 }
                    " Logs"
                }
                if is_done || is_failed {
                    button {
                        class: "btn-icon focus-ring",
                        "aria-label": "Dismiss",
                        onclick: move |_| on_dismiss.call(()),
                        Icon { name: IconName::X, size: 14 }
                    }
                }
            }
            div { class: "deploy-steps",
                for (index, stage) in DEPLOY_STAGES.iter().enumerate() {
                    {
                        let is_past = index < current_idx || is_done;
                        let is_current = index == current_idx && !is_done && !is_failed;
                        let class_name = format!(
                            "deploy-step{}{}",
                            if is_past { " past" } else { "" },
                            if is_current { " current" } else { "" }
                        );
                        rsx! {
                            div { class: "{class_name}",
                                span { class: "deploy-step-dot",
                                    if is_past {
                                        Icon { name: IconName::Check, size: 10 }
                                    } else if is_current {
                                        span { class: "deploy-step-pulse" }
                                    }
                                }
                                span { class: "deploy-step-label", "{stage_label(stage, is_rollback)}" }
                                if index < DEPLOY_STAGES.len() - 1 {
                                    span { class: "deploy-step-bar" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_subtext_describes_pull_based_queue() {
        assert_eq!(
            stage_subtext("queued", false, "host-a", 30, "abc123"),
            "Waiting for host-a agent to check in (heartbeat every 30s)"
        );
    }
}
