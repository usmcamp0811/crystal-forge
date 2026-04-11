//! Modal for editing system configuration.

use crate::api::models::{SystemDetail, UpdateSystemRequest};
use crate::theme;
use dioxus::prelude::*;

#[component]
pub fn EditSystemModal(
    system: SystemDetail,
    on_close: EventHandler<()>,
    on_save: EventHandler<UpdateSystemRequest>,
) -> Element {
    let mut hostname = use_signal(|| system.hostname.clone());
    let mut system_configuration_name =
        use_signal(|| system.system_configuration_name.clone().unwrap_or_default());
    let mut environment = use_signal(|| system.environment.clone().unwrap_or_default());
    let mut deployment_policy = use_signal(|| system.deployment_policy.clone());
    let mut is_saving = use_signal(|| false);

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
            flake_name: None, // Not editable in this version
            deployment_policy: deployment_policy.read().clone(),
        };

        on_save.call(request);
    };

    rsx! {
        // Modal backdrop
        div {
            class: "fixed inset-0 z-50 bg-black/50 p-4 flex items-center justify-center overflow-y-auto",
            onclick: move |_| on_close.call(()),

            // Modal content
            div {
                class: "bg-gray-900 rounded-xl border {theme::surface::CARD_BORDER} shadow-2xl w-full max-w-2xl max-h-[90vh] overflow-hidden flex flex-col",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "px-6 py-4 border-b {theme::surface::CARD_BORDER}",
                    h2 {
                        class: "text-xl font-semibold text-white",
                        "Edit System"
                    }
                    p {
                        class: "text-sm text-gray-400 mt-1",
                        "Update system configuration and deployment settings"
                    }
                }

                // Form
                div {
                    class: "px-6 py-4 space-y-4 overflow-y-auto flex-1",

                    // Hostname
                    div {
                        label {
                            class: "block text-sm font-medium text-gray-300 mb-2",
                            "Hostname"
                        }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 bg-gray-800 border {theme::surface::CARD_BORDER} rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-emerald-500",
                            value: "{hostname}",
                            oninput: move |e| hostname.set(e.value().clone()),
                        }
                    }

                    // System Configuration Name
                    div {
                        label {
                            class: "block text-sm font-medium text-gray-300 mb-2",
                            "System Configuration Name"
                            span { class: "text-gray-500 text-xs ml-2", "(optional)" }
                        }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 bg-gray-800 border {theme::surface::CARD_BORDER} rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-emerald-500",
                            value: "{system_configuration_name}",
                            placeholder: "Defaults to hostname if not set",
                            oninput: move |e| system_configuration_name.set(e.value().clone()),
                        }
                    }

                    // Environment
                    div {
                        label {
                            class: "block text-sm font-medium text-gray-300 mb-2",
                            "Environment"
                            span { class: "text-gray-500 text-xs ml-2", "(optional)" }
                        }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 bg-gray-800 border {theme::surface::CARD_BORDER} rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-emerald-500",
                            value: "{environment}",
                            placeholder: "e.g., production, staging",
                            oninput: move |e| environment.set(e.value().clone()),
                        }
                    }

                    // Deployment Policy
                    div {
                        label {
                            class: "block text-sm font-medium text-gray-300 mb-2",
                            "Deployment Policy"
                        }
                        select {
                            class: "w-full px-4 py-2 bg-gray-800 border {theme::surface::CARD_BORDER} rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-emerald-500",
                            value: "{deployment_policy}",
                            onchange: move |e| deployment_policy.set(e.value().clone()),

                            option { value: "auto_latest", "Auto Latest" }
                            option { value: "manual", "Manual" }
                            option { value: "pinned", "Pinned" }
                        }
                        p {
                            class: "text-xs text-gray-500 mt-2",
                            match deployment_policy.read().as_str() {
                                "auto_latest" => "Automatically deploy the latest commit",
                                "manual" => "Require manual deployment approval",
                                "pinned" => "Deploy only specific pinned commits",
                                _ => ""
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
                        disabled: is_saving(),
                        "Cancel"
                    }

                    button {
                        class: "px-4 py-2 bg-emerald-600 hover:bg-emerald-700 text-white rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
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
