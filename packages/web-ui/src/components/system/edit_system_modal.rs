//! Modal for editing system configuration.

use crate::api::models::{SystemDetail, UpdateSystemRequest};
use crate::theme;
use dioxus::prelude::*;

#[component]
pub fn EditSystemModal(
    system: SystemDetail,
    flake_names: Vec<String>,
    #[props(default)] error_message: Option<String>,
    on_close: EventHandler<()>,
    on_save: EventHandler<UpdateSystemRequest>,
) -> Element {
    let mut hostname = use_signal(|| system.hostname.clone());
    let mut system_configuration_name =
        use_signal(|| system.system_configuration_name.clone().unwrap_or_default());
    let mut environment = use_signal(|| system.environment.clone().unwrap_or_default());
    let mut deployment_policy = use_signal(|| system.deployment_policy.clone());
    let mut flake_name = use_signal(|| {
        system
            .flake
            .as_ref()
            .map(|flake| flake.name.clone())
            .unwrap_or_default()
    });
    let mut is_saving = use_signal(|| false);

    {
        let error_message = error_message.clone();
        use_effect(move || {
            if error_message.is_some() {
                is_saving.set(false);
            }
        });
    }

    let handle_save = move |_| {
        is_saving.set(true);

        let request = UpdateSystemRequest {
            hostname: hostname.read().clone(),
            system_configuration_name: if system_configuration_name.read().trim().is_empty() {
                None
            } else {
                Some(system_configuration_name.read().clone())
            },
            environment: if environment.read().trim().is_empty() {
                None
            } else {
                Some(environment.read().clone())
            },
            flake_name: if flake_name.read().trim().is_empty() {
                None
            } else {
                Some(flake_name.read().clone())
            },
            deployment_policy: deployment_policy.read().clone(),
        };

        on_save.call(request);
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
                        "Edit {system.hostname}"
                    }
                    p {
                        "Update system registration, flake assignment, and deployment policy."
                    }
                }

                div {
                    class: "modal-body",
                    style: "overflow-y:auto;",

                    // Hostname
                    div {
                        label {
                            class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                            "Hostname"
                        }
                        input {
                            r#type: "text",
                            class: "input focus-ring mono",
                            value: "{hostname}",
                            oninput: move |e| hostname.set(e.value().clone()),
                        }
                    }

                    // Flake Name
                    div {
                        label {
                            class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                            "Flake"
                        }
                        select {
                            class: "input focus-ring",
                            onchange: move |e| flake_name.set(e.value().clone()),
                            option {
                                value: "",
                                selected: flake_name.read().is_empty(),
                                "— none —"
                            }
                            for name in flake_names {
                                option {
                                    value: "{name}",
                                    selected: *flake_name.read() == name,
                                    "{name}"
                                }
                            }
                        }
                        if flake_name.read().is_empty() {
                            p {
                                class: "text-xs text-amber-400 mt-1",
                                "⚠ No flake linked — this system won't be included in evaluations."
                            }
                        }
                    }

                    // System Configuration Name
                    div {
                        label {
                            class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                            "System Configuration Name"
                            span { class: "text-gray-500 text-xs ml-2", "(optional)" }
                        }
                        input {
                            r#type: "text",
                            class: "input focus-ring mono",
                            value: "{system_configuration_name}",
                            placeholder: "Defaults to hostname if not set",
                            oninput: move |e| system_configuration_name.set(e.value().clone()),
                        }
                    }

                    // Environment
                    div {
                        label {
                            class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                            "Environment"
                            span { class: "text-gray-500 text-xs ml-2", "(optional)" }
                        }
                        input {
                            r#type: "text",
                            class: "input focus-ring",
                            value: "{environment}",
                            placeholder: "e.g., production, staging",
                            oninput: move |e| environment.set(e.value().clone()),
                        }
                    }

                    // Deployment Policy
                    div {
                        label {
                            class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                            "Deployment Policy"
                        }
                        select {
                            class: "input focus-ring",
                            onchange: move |e| deployment_policy.set(e.value().clone()),
                            option {
                                value: "auto_latest",
                                selected: *deployment_policy.read() == "auto_latest",
                                "Auto Latest"
                            }
                            option {
                                value: "manual",
                                selected: *deployment_policy.read() == "manual",
                                "Manual"
                            }
                            option {
                                value: "pinned",
                                selected: *deployment_policy.read() == "pinned",
                                "Pinned"
                            }
                        }
                        p {
                            class: "text-xs {theme::text::SECONDARY} mt-1",
                            match deployment_policy.read().as_str() {
                                "auto_latest" => "Automatically deploy the latest commit",
                                "manual" => "Require manual deployment approval",
                                "pinned" => "Deploy only specific pinned commits",
                                _ => ""
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
                        disabled: is_saving(),
                        "Cancel"
                    }

                    button {
                        class: "btn btn-primary focus-ring disabled:opacity-50 disabled:cursor-not-allowed",
                        onclick: handle_save,
                        disabled: is_saving() || hostname.read().trim().is_empty(),

                        if is_saving() {
                            "Saving..."
                        } else {
                            "Save Changes"
                        }
                    }
                }
            }
        }
    }
}
