//! Confirmation state and UI for manual deployment on `auto_latest` systems.

use crate::api::models::{DeploySystemRequest, ManualDeploymentAction};
use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

const CANCEL_LABEL: &str = "Cancel";
const CONTINUE_LABEL: &str = "Continue on auto_latest";
const CONVERT_LABEL: &str = "Convert to manual and deploy";

/// State for the `auto_latest` manual-deployment confirmation workflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoLatestDeployState {
    /// No confirmation is visible.
    Closed,
    /// The operator must choose how to handle the persisted policy.
    Confirming {
        /// Exact deployment target retained while the operator chooses.
        commit_sha: String,
    },
    /// A typed deployment request is in progress.
    Submitting {
        /// Exact deployment target submitted to the server.
        commit_sha: String,
        /// Explicit policy behavior submitted with the target.
        action: ManualDeploymentAction,
    },
}

/// Event accepted by [`reduce_auto_latest_deploy_state`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoLatestDeployEvent {
    /// Opens confirmation for an exact commit.
    Open(String),
    /// Closes confirmation without a deployment request.
    Cancel,
    /// Submits one of the two explicit `auto_latest` actions.
    Submit(ManualDeploymentAction),
}

/// Applies one deterministic confirmation-workflow transition.
pub fn reduce_auto_latest_deploy_state(
    state: &AutoLatestDeployState,
    event: AutoLatestDeployEvent,
) -> AutoLatestDeployState {
    match (state, event) {
        (_, AutoLatestDeployEvent::Open(commit_sha)) => {
            AutoLatestDeployState::Confirming { commit_sha }
        }
        (_, AutoLatestDeployEvent::Cancel) => AutoLatestDeployState::Closed,
        (
            AutoLatestDeployState::Confirming { commit_sha },
            AutoLatestDeployEvent::Submit(action),
        ) if matches!(
            action,
            ManualDeploymentAction::ContinueAutoLatest | ManualDeploymentAction::ConvertToManual
        ) =>
        {
            AutoLatestDeployState::Submitting {
                commit_sha: commit_sha.clone(),
                action,
            }
        }
        _ => state.clone(),
    }
}

/// Creates a new deployment intent or reuses an exact failed intent.
pub fn deployment_request_for_target(
    commit_sha: &str,
    action: ManualDeploymentAction,
    retry: Option<&DeploySystemRequest>,
) -> DeploySystemRequest {
    retry
        .filter(|request| request.commit_sha == commit_sha)
        .cloned()
        .unwrap_or_else(|| DeploySystemRequest {
            commit_sha: commit_sha.to_string(),
            action,
            request_id: Some(uuid::Uuid::new_v4()),
        })
}

/// Shows the three required outcomes before manually deploying `auto_latest`.
#[component]
pub fn AutoLatestDeployPrompt(
    commit_sha: String,
    submitting: bool,
    submitting_action: Option<ManualDeploymentAction>,
    on_cancel: EventHandler<()>,
    on_submit: EventHandler<ManualDeploymentAction>,
) -> Element {
    const DIALOG_ID: &str = "auto-latest-deploy-dialog";
    let short_sha = commit_sha.chars().take(7).collect::<String>();
    let opener = use_hook(|| {
        web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.active_element())
            .and_then(|element| element.dyn_into::<HtmlElement>().ok())
    });
    let opener_for_drop = opener.clone();
    use_drop(move || {
        if let Some(opener) = opener_for_drop.as_ref() {
            let _ = opener.focus();
        }
    });
    use_effect(move || {
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(DIALOG_ID))
            .and_then(|element| element.dyn_into::<HtmlElement>().ok())
        {
            let _ = element.focus();
        }
    });
    use_effect(move || {
        if submitting {
            if let Some(element) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.get_element_by_id(DIALOG_ID))
                .and_then(|element| element.dyn_into::<HtmlElement>().ok())
            {
                let _ = element.focus();
            }
        }
    });
    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| {
                if !submitting {
                    on_cancel.call(())
                }
            },
            section {
                class: "modal",
                id: DIALOG_ID,
                role: "dialog",
                "aria-modal": "true",
                "aria-busy": submitting,
                "aria-labelledby": "auto-latest-deploy-title",
                tabindex: "-1",
                style: "width:min(560px,96vw);",
                onclick: move |event| event.stop_propagation(),
                onkeydown: move |event| {
                    if event.key() == Key::Escape && !submitting {
                        event.prevent_default();
                        on_cancel.call(());
                        return;
                    }
                    if event.key() != Key::Tab {
                        return;
                    }
                    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
                        return;
                    };
                    let Some(dialog) = document.get_element_by_id(DIALOG_ID) else {
                        return;
                    };
                    let Ok(nodes) = dialog.query_selector_all("button:not([disabled])") else {
                        return;
                    };
                    let focusable = (0..nodes.length())
                        .filter_map(|index| nodes.item(index))
                        .filter_map(|element| element.dyn_into::<HtmlElement>().ok())
                        .collect::<Vec<_>>();
                    let (Some(first), Some(last)) = (focusable.first(), focusable.last()) else {
                        event.prevent_default();
                        if let Some(dialog) = dialog.dyn_ref::<HtmlElement>() {
                            let _ = dialog.focus();
                        }
                        return;
                    };
                    let active = document.active_element();
                    if event.modifiers().shift() && active.as_ref() == Some(first.as_ref()) {
                        event.prevent_default();
                        let _ = last.focus();
                    } else if !event.modifiers().shift() && active.as_ref() == Some(last.as_ref()) {
                        event.prevent_default();
                        let _ = first.focus();
                    }
                },
                div { class: "modal-head",
                    h2 { id: "auto-latest-deploy-title", "Deploy while auto_latest is enabled?" }
                    p {
                        "Commit " span { class: "mono", "{short_sha}" }
                        " can deploy once without changing policy, or the system can switch to manual first."
                    }
                }
                div { class: "modal-body",
                    div { class: "sd-callout sd-callout-warn",
                        "Converting the policy is persistent. If deployment then fails, the system remains manual."
                    }
                }
                div { class: "modal-foot", style: "flex-wrap:wrap;",
                    button {
                        class: "btn btn-ghost focus-ring",
                        "aria-disabled": submitting,
                        onclick: move |_| if !submitting { on_cancel.call(()) },
                        "{CANCEL_LABEL}"
                    }
                    button {
                        class: "btn btn-ghost focus-ring",
                        "aria-disabled": submitting,
                        onclick: move |_| if !submitting { on_submit.call(ManualDeploymentAction::ContinueAutoLatest) },
                        if submitting && submitting_action == Some(ManualDeploymentAction::ContinueAutoLatest) { "Deploying on auto_latest..." } else { "{CONTINUE_LABEL}" }
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        "aria-disabled": submitting,
                        onclick: move |_| if !submitting { on_submit.call(ManualDeploymentAction::ConvertToManual) },
                        if submitting && submitting_action == Some(ManualDeploymentAction::ConvertToManual) { "Converting and deploying..." } else { "{CONVERT_LABEL}" }
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
    fn reducer_preserves_target_for_each_submit_choice() {
        let confirming = reduce_auto_latest_deploy_state(
            &AutoLatestDeployState::Closed,
            AutoLatestDeployEvent::Open("abcdef012345".to_string()),
        );
        assert_eq!(
            reduce_auto_latest_deploy_state(
                &confirming,
                AutoLatestDeployEvent::Submit(ManualDeploymentAction::ContinueAutoLatest),
            ),
            AutoLatestDeployState::Submitting {
                commit_sha: "abcdef012345".to_string(),
                action: ManualDeploymentAction::ContinueAutoLatest,
            }
        );
        assert_eq!(
            reduce_auto_latest_deploy_state(
                &confirming,
                AutoLatestDeployEvent::Submit(ManualDeploymentAction::ConvertToManual),
            ),
            AutoLatestDeployState::Submitting {
                commit_sha: "abcdef012345".to_string(),
                action: ManualDeploymentAction::ConvertToManual,
            }
        );
    }

    #[test]
    fn reducer_cancel_never_submits() {
        let confirming = AutoLatestDeployState::Confirming {
            commit_sha: "abcdef0".to_string(),
        };
        assert_eq!(
            reduce_auto_latest_deploy_state(&confirming, AutoLatestDeployEvent::Cancel),
            AutoLatestDeployState::Closed
        );
    }

    #[test]
    fn prompt_exposes_all_required_action_labels() {
        assert_eq!(CANCEL_LABEL, "Cancel");
        assert_eq!(CONTINUE_LABEL, "Continue on auto_latest");
        assert_eq!(CONVERT_LABEL, "Convert to manual and deploy");
    }

    #[test]
    fn failed_intent_retry_preserves_request_id_and_original_action() {
        let original = deployment_request_for_target(
            "abcdef0123456789abcdef0123456789abcdef01",
            ManualDeploymentAction::ConvertToManual,
            None,
        );
        let retry = deployment_request_for_target(
            &original.commit_sha,
            ManualDeploymentAction::Deploy,
            Some(&original),
        );

        assert_eq!(retry, original);
        assert_eq!(retry.action, ManualDeploymentAction::ConvertToManual);
        assert!(retry.request_id.is_some());
    }
}
