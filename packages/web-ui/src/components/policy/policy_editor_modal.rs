//! Policy editor modal for creating and editing policy definitions.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::client::{create_deployment_policy, update_deployment_policy};
use crate::api::models::{CreateDeploymentPolicyRequest, UpdateDeploymentPolicyRequest};
use crate::theme;
use crate::views::policies_api;

use super::types::{PolicyDefinition, PolicyFormat};

const CUSTOM_CHECK_JSON_TEMPLATE: &str = r#"{
  "policy_type": "custom_check",
  "config": {
    "expression": "config.networking.firewall.enable",
    "description": "Firewall must be enabled",
    "strict": true
  }
}"#;

const REQUIRE_PACKAGES_JSON_TEMPLATE: &str = r#"{
  "policy_type": "require_packages",
  "config": {
    "packages": ["git", "vim"],
    "strict": true
  }
}"#;

const REQUIRE_SERVICE_JSON_TEMPLATE: &str = r#"{
  "policy_type": "custom_check",
  "config": {
    "expression": "config.services.openssh.enable",
    "description": "OpenSSH service must be enabled",
    "strict": true
  }
}"#;

#[derive(Clone, Copy, PartialEq, Eq)]
enum BasicPolicyKind {
    CustomCheck,
    RequirePackages,
    RequireService,
}

fn parse_policy_payload(
    body: &str,
    format: PolicyFormat,
) -> Result<(String, serde_json::Value), String> {
    let normalize_policy_type = |raw: &str| match raw {
        "require_crystal_forge_agent" => "require_cf_agent".to_string(),
        other => other.to_string(),
    };

    match format {
        PolicyFormat::Json => {
            let value: serde_json::Value =
                serde_json::from_str(body).map_err(|e| format!("Invalid JSON body: {e}"))?;

            let obj = value
                .as_object()
                .ok_or_else(|| "JSON body must be an object".to_string())?;

            let policy_type = obj
                .get("policy_type")
                .or_else(|| obj.get("type"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "JSON body must include 'policy_type' (or 'type')".to_string())?;

            let config = obj.get("config").cloned().unwrap_or_else(|| {
                let mut cloned = obj.clone();
                cloned.remove("policy_type");
                cloned.remove("type");
                cloned.remove("enabled");
                serde_json::Value::Object(cloned)
            });

            Ok((normalize_policy_type(policy_type), config))
        }
        PolicyFormat::Toml => {
            let mut in_policy_block = false;
            let mut json_map = serde_json::Map::new();

            for raw_line in body.lines() {
                let line = raw_line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                if line == "[[policy]]" {
                    if in_policy_block && !json_map.is_empty() {
                        break;
                    }
                    in_policy_block = true;
                    continue;
                }

                if !in_policy_block {
                    continue;
                }

                let Some((key_raw, value_raw)) = line.split_once('=') else {
                    continue;
                };

                let key = key_raw.trim().to_string();
                let value_str = value_raw.trim();

                let value = if value_str.starts_with('"')
                    && value_str.ends_with('"')
                    && value_str.len() >= 2
                {
                    serde_json::Value::String(value_str[1..value_str.len() - 1].to_string())
                } else if value_str == "true" || value_str == "false" {
                    serde_json::Value::Bool(value_str == "true")
                } else if value_str.starts_with('[') && value_str.ends_with(']') {
                    let inner = &value_str[1..value_str.len() - 1];
                    let items = inner
                        .split(',')
                        .map(|part| part.trim())
                        .filter(|part| !part.is_empty())
                        .map(|part| serde_json::Value::String(part.trim_matches('"').to_string()))
                        .collect::<Vec<_>>();
                    serde_json::Value::Array(items)
                } else {
                    serde_json::Value::String(value_str.to_string())
                };

                json_map.insert(key, value);
            }

            let policy_type = json_map
                .get("policy_type")
                .or_else(|| json_map.get("type"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "TOML policy must include 'type'".to_string())?
                .to_string();

            json_map.remove("policy_type");
            json_map.remove("type");
            json_map.remove("enabled");

            Ok((
                normalize_policy_type(&policy_type),
                serde_json::Value::Object(json_map),
            ))
        }
    }
}

/// Modal for creating or editing a policy definition.
#[component]
pub fn PolicyEditorModal(
    editing_policy_id: Signal<Option<Uuid>>,
    edit_name: Signal<String>,
    edit_description: Signal<String>,
    edit_body: Signal<String>,
    edit_format: Signal<PolicyFormat>,
    policy_library: Signal<Vec<PolicyDefinition>>,
    on_close: EventHandler<()>,
) -> Element {
    let is_editing = editing_policy_id.read().is_some();
    let title = if is_editing {
        "Edit Policy"
    } else {
        "Create Policy"
    };
    let action_label = if is_editing {
        "Save Changes"
    } else {
        "Create Policy"
    };
    let mut save_error = use_signal(String::new);
    let mut is_saving = use_signal(|| false);
    let mut advanced_mode = use_signal(|| is_editing);
    let mut basic_kind = use_signal(|| BasicPolicyKind::CustomCheck);
    let mut basic_expression = use_signal(String::new);
    let mut basic_rule_description = use_signal(|| "Custom rule".to_string());
    let mut basic_packages = use_signal(|| "git, vim".to_string());
    let mut basic_service_option = use_signal(|| "config.services.openssh.enable".to_string());
    let mut basic_strict = use_signal(|| true);
    let mut show_strict_info = use_signal(|| false);
    let current_validation_error = {
        let name = edit_name.read().trim().to_string();
        if name.is_empty() {
            Some("Policy name is required".to_string())
        } else if !*advanced_mode.read() {
            match *basic_kind.read() {
                BasicPolicyKind::CustomCheck => {
                    if basic_expression.read().trim().is_empty() {
                        Some("Custom check expression is required in Basic mode".to_string())
                    } else {
                        None
                    }
                }
                BasicPolicyKind::RequirePackages => {
                    if basic_packages.read().trim().is_empty() {
                        Some("At least one package is required in Basic mode".to_string())
                    } else {
                        None
                    }
                }
                BasicPolicyKind::RequireService => {
                    if basic_service_option.read().trim().is_empty() {
                        Some("Service option path is required in Basic mode".to_string())
                    } else {
                        None
                    }
                }
            }
        } else {
            let body = edit_body.read().clone();
            let format = *edit_format.read();
            parse_policy_payload(&body, format).err()
        }
    };
    let can_save = current_validation_error.is_none() && !*is_saving.read();

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-start sm:items-center justify-center p-2 sm:p-4 cf-modal-overlay-z50 overflow-y-auto",
            onclick: move |_| on_close.call(()),

            div {
                class: "{theme::surface::CARD_BG} border border-violet-500/30 rounded-2xl p-3 shadow-xl shadow-violet-900/20 cf-modal-panel-wide w-full max-w-5xl max-h-[78vh] flex flex-col",
                onclick: |evt| evt.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between gap-3 shrink-0",
                    div {
                        class: "flex items-center gap-3",
                        div {
                            class: "w-8 h-8 rounded-lg bg-violet-500/20 flex items-center justify-center",
                            svg {
                                class: "w-4 h-4 text-violet-400",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                                }
                            }
                        }
                        div {
                            h3 { class: "text-white text-lg font-semibold", "{title}" }
                            p { class: "text-xs {theme::text::MUTED}", "Define metadata and policy payload." }
                        }
                    }
                    div {
                        class: "flex items-center gap-2",
                        div {
                            class: "inline-flex rounded-md border border-gray-700 bg-gray-950/50 p-1",
                            button {
                                class: "px-2 py-1 rounded text-xs transition-colors",
                                class: if !*advanced_mode.read() {
                                    "bg-violet-500/20 text-violet-300"
                                } else {
                                    "text-gray-400 hover:text-gray-200"
                                },
                                onclick: move |_| advanced_mode.set(false),
                                "Basic"
                            }
                            button {
                                class: "px-2 py-1 rounded text-xs transition-colors",
                                class: if *advanced_mode.read() {
                                    "bg-violet-500/20 text-violet-300"
                                } else {
                                    "text-gray-400 hover:text-gray-200"
                                },
                                onclick: move |_| advanced_mode.set(true),
                                "Advanced"
                            }
                        }
                        button {
                            class: "p-2 rounded-lg text-gray-400 hover:text-white hover:bg-violet-500/10 transition-colors",
                            onclick: move |_| on_close.call(()),
                            svg {
                                class: "w-5 h-5",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M6 18L18 6M6 6l12 12"
                                }
                            }
                        }
                    }
                }

                // Form content
                div {
                    class: if *advanced_mode.read() {
                        "grid grid-cols-1 lg:grid-cols-[230px_1fr] gap-3 items-start mt-3 flex-1 min-h-0 overflow-y-auto pr-1"
                    } else {
                        "grid grid-cols-1 gap-2 items-start mt-2 flex-1 min-h-0 overflow-y-auto pr-1"
                    },

                    // Left column - metadata
                    div {
                        class: "space-y-3 min-h-0",
                        div {
                            class: "space-y-2",
                            label { class: "text-xs text-violet-300/70 font-medium", "Policy Name" }
                            input {
                                class: "w-full rounded-lg border border-gray-700 bg-gray-950/50 px-3 py-2 text-sm text-gray-100 focus:outline-none focus:ring-2 focus:ring-violet-500/40 focus:border-violet-500/50",
                                placeholder: "e.g., Require SSH Enabled",
                                value: "{edit_name}",
                                oninput: move |event| {
                                    edit_name.set(event.value());
                                    save_error.set(String::new());
                                },
                            }
                        }
                        div {
                            class: "space-y-2",
                            label { class: "text-xs text-violet-300/70 font-medium", "Description" }
                            textarea {
                                class: "w-full rounded-lg border border-gray-700 bg-gray-950/50 px-3 py-2 text-sm text-gray-100 focus:outline-none focus:ring-2 focus:ring-violet-500/40 focus:border-violet-500/50 resize-none",
                                placeholder: "Describe what this policy enforces...",
                                rows: "3",
                                value: "{edit_description}",
                                oninput: move |event| {
                                    edit_description.set(event.value());
                                    save_error.set(String::new());
                                },
                            }
                        }
                        if *advanced_mode.read() {
                            div {
                                class: "space-y-2",
                                label { class: "text-xs text-violet-300/70 font-medium", "Format" }
                                div {
                                    class: "flex gap-2",
                                    button {
                                        class: "px-3 py-1.5 rounded-md text-xs border transition-colors",
                                        class: if *edit_format.read() == PolicyFormat::Toml {
                                            "bg-violet-500/20 border-violet-500 text-violet-300"
                                        } else {
                                            "bg-gray-950/50 border-gray-700 text-gray-400 hover:border-gray-600"
                                        },
                                        onclick: move |_| {
                                            edit_format.set(PolicyFormat::Toml);
                                            save_error.set(String::new());
                                        },
                                        "TOML"
                                    }
                                    button {
                                        class: "px-3 py-1.5 rounded-md text-xs border transition-colors",
                                        class: if *edit_format.read() == PolicyFormat::Json {
                                            "bg-violet-500/20 border-violet-500 text-violet-300"
                                        } else {
                                            "bg-gray-950/50 border-gray-700 text-gray-400 hover:border-gray-600"
                                        },
                                        onclick: move |_| {
                                            edit_format.set(PolicyFormat::Json);
                                            save_error.set(String::new());
                                        },
                                        "JSON"
                                    }
                                }
                            }
                        }
                        div {
                            class: "space-y-2",
                            if *advanced_mode.read() {
                                label { class: "text-xs text-violet-300/70 font-medium", "Templates" }
                                div {
                                    class: "flex flex-wrap gap-2",
                                    button {
                                        class: "px-3 py-1.5 rounded-md text-xs border border-gray-700 text-gray-300 hover:bg-gray-800",
                                        onclick: move |_| {
                                            edit_format.set(PolicyFormat::Json);
                                            edit_body.set(CUSTOM_CHECK_JSON_TEMPLATE.to_string());
                                            save_error.set(String::new());
                                        },
                                        "Custom check"
                                    }
                                    button {
                                        class: "px-3 py-1.5 rounded-md text-xs border border-gray-700 text-gray-300 hover:bg-gray-800",
                                        onclick: move |_| {
                                            edit_format.set(PolicyFormat::Json);
                                            edit_body.set(REQUIRE_PACKAGES_JSON_TEMPLATE.to_string());
                                            save_error.set(String::new());
                                        },
                                        "Require packages"
                                    }
                                    button {
                                        class: "px-3 py-1.5 rounded-md text-xs border border-gray-700 text-gray-300 hover:bg-gray-800",
                                        onclick: move |_| {
                                            edit_format.set(PolicyFormat::Json);
                                            edit_body.set(REQUIRE_SERVICE_JSON_TEMPLATE.to_string());
                                            save_error.set(String::new());
                                        },
                                        "Require service"
                                    }
                                }
                            } else {
                                label { class: "text-xs text-violet-300/70 font-medium", "Policy Type" }
                                div {
                                    class: "flex flex-wrap gap-2",
                                    button {
                                        class: "px-3 py-1.5 rounded-md text-xs border transition-colors",
                                        class: if *basic_kind.read() == BasicPolicyKind::CustomCheck {
                                            "bg-violet-500/20 border-violet-500 text-violet-300"
                                        } else {
                                            "border-gray-700 text-gray-300 hover:bg-gray-800"
                                        },
                                        onclick: move |_| {
                                            basic_kind.set(BasicPolicyKind::CustomCheck);
                                            save_error.set(String::new());
                                        },
                                        "Custom check"
                                    }
                                    button {
                                        class: "px-3 py-1.5 rounded-md text-xs border transition-colors",
                                        class: if *basic_kind.read() == BasicPolicyKind::RequirePackages {
                                            "bg-violet-500/20 border-violet-500 text-violet-300"
                                        } else {
                                            "border-gray-700 text-gray-300 hover:bg-gray-800"
                                        },
                                        onclick: move |_| {
                                            basic_kind.set(BasicPolicyKind::RequirePackages);
                                            save_error.set(String::new());
                                        },
                                        "Require packages"
                                    }
                                    button {
                                        class: "px-3 py-1.5 rounded-md text-xs border transition-colors",
                                        class: if *basic_kind.read() == BasicPolicyKind::RequireService {
                                            "bg-violet-500/20 border-violet-500 text-violet-300"
                                        } else {
                                            "border-gray-700 text-gray-300 hover:bg-gray-800"
                                        },
                                        onclick: move |_| {
                                            basic_kind.set(BasicPolicyKind::RequireService);
                                            save_error.set(String::new());
                                        },
                                        "Require service"
                                    }
                                }

                                if *basic_kind.read() == BasicPolicyKind::CustomCheck {
                                    div { class: "space-y-2",
                                        label { class: "text-xs text-violet-300/70 font-medium", "Expression" }
                                        input {
                                            class: "w-full rounded-lg border border-gray-700 bg-gray-950/50 px-3 py-2 text-xs text-gray-100 focus:outline-none focus:ring-2 focus:ring-violet-500/40",
                                            placeholder: "config.networking.firewall.enable",
                                            value: "{basic_expression}",
                                            oninput: move |event| {
                                                basic_expression.set(event.value());
                                                save_error.set(String::new());
                                            },
                                        }
                                        label { class: "text-xs text-violet-300/70 font-medium", "Rule description" }
                                        input {
                                            class: "w-full rounded-lg border border-gray-700 bg-gray-950/50 px-3 py-2 text-xs text-gray-100 focus:outline-none focus:ring-2 focus:ring-violet-500/40",
                                            value: "{basic_rule_description}",
                                            oninput: move |event| {
                                                basic_rule_description.set(event.value());
                                                save_error.set(String::new());
                                            },
                                        }
                                    }
                                }

                                if *basic_kind.read() == BasicPolicyKind::RequirePackages {
                                    div { class: "space-y-2",
                                        label { class: "text-xs text-violet-300/70 font-medium", "Packages (comma separated)" }
                                        input {
                                            class: "w-full rounded-lg border border-gray-700 bg-gray-950/50 px-3 py-2 text-xs text-gray-100 focus:outline-none focus:ring-2 focus:ring-violet-500/40",
                                            placeholder: "git, vim, htop",
                                            value: "{basic_packages}",
                                            oninput: move |event| {
                                                basic_packages.set(event.value());
                                                save_error.set(String::new());
                                            },
                                        }
                                    }
                                }

                                if *basic_kind.read() == BasicPolicyKind::RequireService {
                                    div { class: "space-y-2",
                                        label { class: "text-xs text-violet-300/70 font-medium", "Service option path" }
                                        input {
                                            class: "w-full rounded-lg border border-gray-700 bg-gray-950/50 px-3 py-2 text-xs text-gray-100 focus:outline-none focus:ring-2 focus:ring-violet-500/40",
                                            placeholder: "config.services.openssh.enable",
                                            value: "{basic_service_option}",
                                            oninput: move |event| {
                                                basic_service_option.set(event.value());
                                                save_error.set(String::new());
                                            },
                                        }
                                    }
                                }

                                div {
                                    class: "flex items-center gap-2",
                                    input {
                                        r#type: "checkbox",
                                        checked: *basic_strict.read(),
                                        onchange: move |_| {
                                            let next = {
                                                let current = *basic_strict.read();
                                                !current
                                            };
                                            basic_strict.set(next);
                                            save_error.set(String::new());
                                        }
                                    }
                                    span { class: "text-xs text-gray-300", "Strict mode" }
                                    button {
                                        class: "w-4 h-4 rounded-full border border-gray-600 text-[10px] text-gray-300 hover:text-white hover:border-gray-400 inline-flex items-center justify-center",
                                        onclick: move |_| {
                                            let next = {
                                                let current = *show_strict_info.read();
                                                !current
                                            };
                                            show_strict_info.set(next);
                                        },
                                        "i"
                                    }
                                }
                                if *show_strict_info.read() {
                                    div {
                                        class: "text-[11px] {theme::text::MUTED} bg-gray-900/60 border border-gray-700 rounded px-2 py-1",
                                        "Strict mode fails evaluation when this check is false. Non-strict mode records the result but does not fail overall evaluation."
                                    }
                                }
                            }
                        }

                        if !*advanced_mode.read() {
                            div {
                                class: "rounded-lg border border-gray-700 bg-gray-950/40 p-3 space-y-1",
                                p { class: "text-xs text-gray-300", "Basic mode keeps policy creation compact." }
                                p { class: "text-xs {theme::text::MUTED}", "Use Advanced for free-form JSON/TOML." }
                            }
                        }
                    }

                    // Right column - code editor
                    if *advanced_mode.read() {
                        div {
                        class: "space-y-2 flex flex-col min-h-0",
                            label { class: "text-xs text-violet-300/70 font-medium", "Policy Definition" }
                            div {
                                class: "rounded-lg border border-gray-700 bg-gray-950/70 overflow-hidden flex-1 min-h-0",
                                textarea {
                                    class: "w-full bg-transparent px-3 py-2 text-xs text-gray-100 font-mono focus:outline-none resize-none",
                                    style: "height: clamp(100px, 20vh, 180px);",
                                    rows: "8",
                                    value: "{edit_body}",
                                    oninput: move |event| {
                                        edit_body.set(event.value());
                                        save_error.set(String::new());
                                    },
                                    spellcheck: "false",
                                }
                            }
                            p {
                                class: "text-[11px] {theme::text::MUTED}",
                                "Tip: prefer JSON with policy_type + config for reliable saves."
                            }
                        }
                    }

                    if let Some(message) = current_validation_error.clone() {
                        div {
                            class: "text-xs text-amber-300 bg-amber-950/40 border border-amber-700/40 rounded px-3 py-2",
                            "{message}"
                        }
                    }
                    if !save_error.read().is_empty() {
                        div {
                            class: "text-xs text-red-300 bg-red-950/40 border border-red-700/40 rounded px-3 py-2",
                            "{save_error}"
                        }
                    }
                }

                // Footer
                div {
                    class: "flex justify-end items-center gap-3 pt-2 mt-3 border-t border-gray-800 shrink-0",
                    button {
                        class: "px-4 py-2 rounded-lg text-sm text-gray-300 border border-gray-700 hover:bg-gray-800 transition-colors",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 rounded-lg text-sm font-semibold bg-violet-600 hover:bg-violet-500 text-white transition-colors shadow-lg shadow-violet-900/30",
                        disabled: !can_save,
                        onclick: move |_| {
                            let validation_error_for_submit = current_validation_error.clone();
                            let name = edit_name.read().clone();
                            let description = edit_description.read().clone();
                            let body = edit_body.read().clone();
                            let format = *edit_format.read();
                            let in_basic_mode = !*advanced_mode.read();
                            let kind = *basic_kind.read();
                            let expression = basic_expression.read().clone();
                            let rule_description = basic_rule_description.read().clone();
                            let packages_raw = basic_packages.read().clone();
                            let service_option_raw = basic_service_option.read().clone();
                            let strict = *basic_strict.read();
                            let editing_id = *editing_policy_id.read();
                            let mut policy_library = policy_library;
                            let on_close = on_close;
                            let mut save_error = save_error;
                            let mut is_saving = is_saving;

                            if let Some(message) = validation_error_for_submit {
                                save_error.set(message);
                                return;
                            }
                            save_error.set(String::new());
                            is_saving.set(true);

                            spawn(async move {
                                let (policy_type, config) = if in_basic_mode {
                                    match kind {
                                        BasicPolicyKind::CustomCheck => {
                                            let cfg = serde_json::json!({
                                                "expression": expression,
                                                "description": rule_description,
                                                "strict": strict,
                                            });
                                            ("custom_check".to_string(), cfg)
                                        }
                                        BasicPolicyKind::RequirePackages => {
                                            let packages = packages_raw
                                                .split(',')
                                                .map(|p| p.trim())
                                                .filter(|p| !p.is_empty())
                                                .map(|p| p.to_string())
                                                .collect::<Vec<_>>();
                                            let cfg = serde_json::json!({
                                                "packages": packages,
                                                "strict": strict,
                                            });
                                            ("require_packages".to_string(), cfg)
                                        }
                                        BasicPolicyKind::RequireService => {
                                            let option = service_option_raw.trim().to_string();
                                            let cfg = serde_json::json!({
                                                "expression": option,
                                                "description": format!("Service option must be enabled: {option}"),
                                                "strict": strict,
                                            });
                                            ("custom_check".to_string(), cfg)
                                        }
                                    }
                                } else {
                                    let parsed = parse_policy_payload(&body, format);
                                    match parsed {
                                        Ok(values) => values,
                                        Err(message) => {
                                            save_error.set(format!("Policy parse error: {message}"));
                                            is_saving.set(false);
                                            return;
                                        }
                                    }
                                };

                                let result = if let Some(policy_id) = editing_id {
                                    let request = UpdateDeploymentPolicyRequest {
                                        name: Some(name.clone()),
                                        description: Some(description.clone()),
                                        policy_type: Some(policy_type),
                                        config: Some(config),
                                        enabled: Some(true),
                                    };
                                    update_deployment_policy(&policy_id, &request)
                                        .await
                                        .map(|_| ())
                                } else {
                                    let request = CreateDeploymentPolicyRequest {
                                        name: name.clone(),
                                        description: Some(description.clone()),
                                        policy_type,
                                        config,
                                        enabled: Some(true),
                                    };
                                    create_deployment_policy(&request).await.map(|_| ())
                                };

                                match result {
                                    Ok(()) => {
                                        let latest = policies_api::load_policies_with_fallback().await;
                                        policy_library.set(latest);
                                        is_saving.set(false);
                                        on_close.call(());
                                    }
                                    Err(error) => {
                                        save_error.set(format!("Failed to save policy: {error}"));
                                        is_saving.set(false);
                                    }
                                }
                            });
                        },
                        if *is_saving.read() { "Saving..." } else { "{action_label}" }
                    }
                }
            }
        }
    }
}
