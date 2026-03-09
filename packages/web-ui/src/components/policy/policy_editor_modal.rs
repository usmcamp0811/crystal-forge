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

#[derive(Clone, Copy, PartialEq, Eq)]
enum BasicPolicyKind {
    CustomCheck,
    RequirePackages,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BasicCustomBuilder {
    CustomExpression,
    ServiceEnabled,
    FirewallPortAllowed,
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

fn toml_literal(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Array(arr) => {
            let items = arr
                .iter()
                .filter_map(|v| {
                    v.as_str()
                        .map(|s| format!("\"{}\"", s.replace('"', "\\\"")))
                })
                .collect::<Vec<_>>();
            format!("[{}]", items.join(", "))
        }
        _ => format!("\"{}\"", value),
    }
}

fn format_policy_payload(
    policy_type: &str,
    config: &serde_json::Value,
    format: PolicyFormat,
) -> String {
    match format {
        PolicyFormat::Json => serde_json::to_string_pretty(&serde_json::json!({
            "policy_type": policy_type,
            "config": config,
        }))
        .unwrap_or_else(|_| "{}".to_string()),
        PolicyFormat::Toml => {
            let mut out = String::from("[[policy]]\n");
            out.push_str(&format!("type = \"{}\"\n", policy_type));

            if let Some(obj) = config.as_object() {
                for (k, v) in obj {
                    out.push_str(&format!("{} = {}\n", k, toml_literal(v)));
                }
            }

            out
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
    let initial_parsed = parse_policy_payload(&edit_body.read().clone(), *edit_format.read()).ok();
    let initial_policy_type = initial_parsed
        .as_ref()
        .map(|(policy_type, _)| policy_type.as_str())
        .unwrap_or("custom_check");
    let initial_config = initial_parsed
        .as_ref()
        .map(|(_, config)| config.clone())
        .unwrap_or_else(|| serde_json::json!({}));
    let initial_expression = initial_config
        .get("expression")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let mut save_error = use_signal(String::new);
    let mut is_saving = use_signal(|| false);
    let mut advanced_mode = use_signal(|| false);
    let mut basic_kind = use_signal(|| {
        if initial_policy_type == "require_packages" {
            BasicPolicyKind::RequirePackages
        } else {
            BasicPolicyKind::CustomCheck
        }
    });
    let mut basic_custom_builder = use_signal(|| {
        if initial_expression.contains("builtins.elem")
            && initial_expression.contains("config.networking.firewall")
        {
            BasicCustomBuilder::FirewallPortAllowed
        } else if (initial_expression.starts_with("config.services.")
            || initial_expression.starts_with("!config.services."))
            && initial_expression.ends_with(".enable")
        {
            BasicCustomBuilder::ServiceEnabled
        } else {
            BasicCustomBuilder::CustomExpression
        }
    });
    let mut basic_expression = use_signal(|| initial_expression.clone());
    let mut basic_rule_description = use_signal(|| {
        initial_config
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    });
    let mut basic_packages = use_signal(|| {
        initial_config
            .get("packages")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    });
    let mut basic_service_name = use_signal(|| {
        initial_expression
            .trim_start_matches('!')
            .trim_start_matches("config.services.")
            .trim_end_matches(".enable")
            .to_string()
    });
    let mut basic_service_expectation = use_signal(|| {
        if initial_expression.starts_with('!') {
            "disabled".to_string()
        } else {
            "enabled".to_string()
        }
    });
    let mut basic_firewall_port = use_signal(|| {
        initial_expression
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_string()
    });
    let mut basic_firewall_protocol = use_signal(|| {
        if initial_expression.contains("allowedUDPPorts") {
            "udp".to_string()
        } else {
            "tcp".to_string()
        }
    });
    let mut basic_firewall_expectation = use_signal(|| {
        if initial_expression.contains("!builtins.elem") {
            "denied".to_string()
        } else {
            "allowed".to_string()
        }
    });
    let mut basic_strict = use_signal(|| {
        initial_config
            .get("strict")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    });
    let mut show_strict_info = use_signal(|| false);
    let current_validation_error = {
        let name = edit_name.read().trim().to_string();
        if name.is_empty() {
            Some("Policy name is required".to_string())
        } else if !*advanced_mode.read() {
            match *basic_kind.read() {
                BasicPolicyKind::CustomCheck => match *basic_custom_builder.read() {
                    BasicCustomBuilder::CustomExpression => {
                        if basic_expression.read().trim().is_empty() {
                            Some("Custom expression is required in Basic mode".to_string())
                        } else {
                            None
                        }
                    }
                    BasicCustomBuilder::ServiceEnabled => {
                        if basic_service_name.read().trim().is_empty() {
                            Some("Service name is required in Basic mode".to_string())
                        } else {
                            None
                        }
                    }
                    BasicCustomBuilder::FirewallPortAllowed => {
                        let port_text = basic_firewall_port.read().trim().to_string();
                        if port_text.is_empty() {
                            Some("Firewall port is required in Basic mode".to_string())
                        } else if port_text.parse::<u16>().map(|p| p == 0).unwrap_or(true) {
                            Some("Firewall port must be a valid number (1-65535)".to_string())
                        } else {
                            None
                        }
                    }
                },
                BasicPolicyKind::RequirePackages => {
                    if basic_packages.read().trim().is_empty() {
                        Some("At least one package is required in Basic mode".to_string())
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
    let name_missing_error = current_validation_error
        .as_ref()
        .map(|s| s == "Policy name is required")
        .unwrap_or(false);
    let non_name_validation_error = current_validation_error
        .as_ref()
        .filter(|s| s.as_str() != "Policy name is required")
        .cloned();
    let can_save = current_validation_error.is_none() && !*is_saving.read();

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-start sm:items-center justify-center p-2 sm:p-3 cf-modal-overlay-z50 overflow-y-auto",
            onclick: move |_| on_close.call(()),

            div {
                class: "{theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} rounded-xl p-3 shadow-2xl cf-modal-panel-wide cf-policy-modal-panel w-full max-w-5xl flex flex-col overflow-hidden",
                style: "max-height: calc(100dvh - 1rem);",
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
                            p { class: "text-[11px] {theme::text::MUTED}", "Policy metadata + payload" }
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
                                onclick: move |_| {
                                    if *advanced_mode.read() {
                                        let body = edit_body.read().clone();
                                        let format = *edit_format.read();
                                        if let Ok((policy_type, config)) = parse_policy_payload(&body, format) {
                                            let strict = config
                                                .get("strict")
                                                .and_then(|v| v.as_bool())
                                                .unwrap_or(true);
                                            basic_strict.set(strict);

                                            if policy_type == "require_packages" {
                                                basic_kind.set(BasicPolicyKind::RequirePackages);
                                                let packages = config
                                                    .get("packages")
                                                    .and_then(|v| v.as_array())
                                                    .map(|arr| {
                                                        arr.iter()
                                                            .filter_map(|v| v.as_str())
                                                            .collect::<Vec<_>>()
                                                            .join(", ")
                                                    })
                                                    .unwrap_or_default();
                                                basic_packages.set(packages);
                                            } else {
                                                basic_kind.set(BasicPolicyKind::CustomCheck);
                                                let expression = config
                                                    .get("expression")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or_default()
                                                    .to_string();
                                                let result_message = config
                                                    .get("description")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or_default()
                                                    .to_string();

                                                basic_expression.set(expression.clone());
                                                basic_rule_description.set(result_message);

                                                if (expression.starts_with("config.services.")
                                                    || expression.starts_with("!config.services."))
                                                    && expression.ends_with(".enable")
                                                {
                                                    basic_custom_builder
                                                        .set(BasicCustomBuilder::ServiceEnabled);
                                                    if expression.starts_with('!') {
                                                        basic_service_expectation
                                                            .set("disabled".to_string());
                                                    } else {
                                                        basic_service_expectation
                                                            .set("enabled".to_string());
                                                    }
                                                    let service_name = expression
                                                        .trim_start_matches('!')
                                                        .trim_start_matches("config.services.")
                                                        .trim_end_matches(".enable")
                                                        .to_string();
                                                    basic_service_name.set(service_name);
                                                } else if expression.contains("builtins.elem")
                                                    && expression.contains("config.networking.firewall")
                                                {
                                                    basic_custom_builder.set(
                                                        BasicCustomBuilder::FirewallPortAllowed,
                                                    );
                                                    if expression.contains("!builtins.elem") {
                                                        basic_firewall_expectation
                                                            .set("denied".to_string());
                                                    } else {
                                                        basic_firewall_expectation
                                                            .set("allowed".to_string());
                                                    }
                                                    let maybe_port = expression
                                                        .split_whitespace()
                                                        .nth(1)
                                                        .unwrap_or_default()
                                                        .to_string();
                                                    basic_firewall_port.set(maybe_port);
                                                    if expression.contains("allowedUDPPorts") {
                                                        basic_firewall_protocol
                                                            .set("udp".to_string());
                                                    } else {
                                                        basic_firewall_protocol
                                                            .set("tcp".to_string());
                                                    }
                                                } else {
                                                    basic_custom_builder
                                                        .set(BasicCustomBuilder::CustomExpression);
                                                }
                                            }
                                        }
                                    }
                                    advanced_mode.set(false)
                                },
                                "Basic"
                            }
                            button {
                                class: "px-2 py-1 rounded text-xs transition-colors",
                                class: if *advanced_mode.read() {
                                    "bg-violet-500/20 text-violet-300"
                                } else {
                                    "text-gray-400 hover:text-gray-200"
                                },
                                onclick: move |_| {
                                    if !*advanced_mode.read() {
                                        let strict = *basic_strict.read();
                                        let (policy_type, config) = match *basic_kind.read() {
                                            BasicPolicyKind::RequirePackages => {
                                                let packages = basic_packages
                                                    .read()
                                                    .split(',')
                                                    .map(|p| p.trim())
                                                    .filter(|p| !p.is_empty())
                                                    .map(|p| p.to_string())
                                                    .collect::<Vec<_>>();
                                                (
                                                    "require_packages".to_string(),
                                                    serde_json::json!({
                                                        "packages": packages,
                                                        "strict": strict,
                                                    }),
                                                )
                                            }
                                            BasicPolicyKind::CustomCheck => {
                                                let (expr, default_msg) =
                                                    match *basic_custom_builder.read() {
                                                        BasicCustomBuilder::CustomExpression => (
                                                            basic_expression
                                                                .read()
                                                                .trim()
                                                                .to_string(),
                                                            "Custom rule failed".to_string(),
                                                        ),
                                                        BasicCustomBuilder::ServiceEnabled => {
                                                            let svc = basic_service_name
                                                                .read()
                                                                .trim()
                                                                .to_string();
                                                            let expectation =
                                                                basic_service_expectation
                                                                    .read()
                                                                    .trim()
                                                                    .to_lowercase();
                                                            let base_expr = format!(
                                                                "config.services.{svc}.enable"
                                                            );
                                                            (
                                                                if expectation == "disabled" {
                                                                    format!("!{base_expr}")
                                                                } else {
                                                                    base_expr
                                                                },
                                                                if expectation == "disabled" {
                                                                    format!(
                                                                        "Service must be disabled: {svc}"
                                                                    )
                                                                } else {
                                                                    format!(
                                                                        "Service must be enabled: {svc}"
                                                                    )
                                                                },
                                                            )
                                                        }
                                                        BasicCustomBuilder::FirewallPortAllowed => {
                                                            let port: u16 = basic_firewall_port
                                                                .read()
                                                                .trim()
                                                                .parse()
                                                                .unwrap_or(0);
                                                            let proto = basic_firewall_protocol
                                                                .read()
                                                                .trim()
                                                                .to_lowercase();
                                                            let list_attr = if proto == "udp" {
                                                                "allowedUDPPorts"
                                                            } else {
                                                                "allowedTCPPorts"
                                                            };
                                                            (
                                                                format!(
                                                                    "builtins.elem {port} (config.networking.firewall.{list_attr} or [])"
                                                                ),
                                                                format!(
                                                                    "Firewall must allow {proto}/{port}"
                                                                ),
                                                            )
                                                        }
                                                    };
                                                let msg = if basic_rule_description
                                                    .read()
                                                    .trim()
                                                    .is_empty()
                                                {
                                                    default_msg
                                                } else {
                                                    basic_rule_description
                                                        .read()
                                                        .trim()
                                                        .to_string()
                                                };
                                                (
                                                    "custom_check".to_string(),
                                                    serde_json::json!({
                                                        "expression": expr,
                                                        "description": msg,
                                                        "strict": strict,
                                                    }),
                                                )
                                            }
                                        };

                                        edit_format.set(PolicyFormat::Json);
                                        edit_body.set(format_policy_payload(
                                            &policy_type,
                                            &config,
                                            PolicyFormat::Json,
                                        ));
                                    }
                                    advanced_mode.set(true)
                                },
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
                        "grid grid-cols-1 lg:grid-cols-[220px_1fr] gap-2 items-start mt-2 flex-1 min-h-0 overflow-y-auto pr-1"
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
                                class: if name_missing_error {
                                    "w-full rounded-lg border px-3 py-2 text-sm cf-policy-modal-field cf-policy-modal-field-error focus:outline-none"
                                } else {
                                    "w-full rounded-lg border px-3 py-2 text-sm cf-policy-modal-field focus:outline-none"
                                },
                                placeholder: "e.g., Require SSH Enabled",
                                value: "{edit_name}",
                                oninput: move |event| {
                                    edit_name.set(event.value());
                                    save_error.set(String::new());
                                },
                            }
                            if name_missing_error {
                                p {
                                    class: "text-[11px] text-red-300",
                                    "Policy name is required"
                                }
                            }
                        }
                        div {
                            class: "space-y-2",
                            label { class: "text-xs text-violet-300/70 font-medium", "Description" }
                            textarea {
                                class: "w-full rounded-lg border px-3 py-2 text-sm cf-policy-modal-field focus:outline-none resize-none",
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
                                            let current_body = edit_body.read().clone();
                                            let current_format = *edit_format.read();
                                            if current_format != PolicyFormat::Toml {
                                                if let Ok((policy_type, config)) = parse_policy_payload(&current_body, current_format) {
                                                    edit_body.set(format_policy_payload(&policy_type, &config, PolicyFormat::Toml));
                                                }
                                            }
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
                                            let current_body = edit_body.read().clone();
                                            let current_format = *edit_format.read();
                                            if current_format != PolicyFormat::Json {
                                                if let Ok((policy_type, config)) = parse_policy_payload(&current_body, current_format) {
                                                    edit_body.set(format_policy_payload(&policy_type, &config, PolicyFormat::Json));
                                                }
                                            }
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
                            if *advanced_mode.read() && !is_editing {
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
                                        "Custom rule"
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
                                }
                            } else {
                                if !is_editing {
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
                                            "Custom rule"
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
                                    }
                                }

                                if *basic_kind.read() == BasicPolicyKind::CustomCheck {
                                    div { class: "space-y-2",
                                        if !is_editing {
                                            div { class: "flex flex-wrap gap-2",
                                                button {
                                                    class: "px-2 py-1 rounded text-[11px] border border-gray-700 text-gray-300 hover:bg-gray-800",
                                                    onclick: move |_| {
                                                        basic_custom_builder.set(BasicCustomBuilder::ServiceEnabled);
                                                        save_error.set(String::new());
                                                    },
                                                    "Service state"
                                                }
                                                button {
                                                    class: "px-2 py-1 rounded text-[11px] border border-gray-700 text-gray-300 hover:bg-gray-800",
                                                    onclick: move |_| {
                                                        basic_custom_builder.set(BasicCustomBuilder::FirewallPortAllowed);
                                                        save_error.set(String::new());
                                                    },
                                                    "Firewall port state"
                                                }
                                                button {
                                                    class: "px-2 py-1 rounded text-[11px] border border-gray-700 text-gray-300 hover:bg-gray-800",
                                                    onclick: move |_| {
                                                        basic_custom_builder.set(BasicCustomBuilder::CustomExpression);
                                                        save_error.set(String::new());
                                                    },
                                                    "Custom expression"
                                                }
                                            }
                                        }

                                        if *basic_custom_builder.read() == BasicCustomBuilder::CustomExpression {
                                            label { class: "text-xs text-violet-300/70 font-medium", "Expression" }
                                            input {
                                                class: "w-full rounded-lg border px-3 py-2 text-xs cf-policy-modal-field focus:outline-none",
                                                placeholder: "config.networking.firewall.enable",
                                                value: "{basic_expression}",
                                                oninput: move |event| {
                                                    basic_expression.set(event.value());
                                                    save_error.set(String::new());
                                                },
                                            }
                                        }

                                        if *basic_custom_builder.read() == BasicCustomBuilder::ServiceEnabled {
                                            label { class: "text-xs text-violet-300/70 font-medium", "Service name" }
                                            input {
                                                class: "w-full rounded-lg border px-3 py-2 text-xs cf-policy-modal-field focus:outline-none",
                                                placeholder: "openssh",
                                                value: "{basic_service_name}",
                                                oninput: move |event| {
                                                    basic_service_name.set(event.value());
                                                    save_error.set(String::new());
                                                },
                                            }
                                            div { class: "inline-flex rounded-md border border-gray-700 bg-gray-950/50 p-1",
                                                button {
                                                    class: "px-2 py-1 rounded text-[11px] transition-colors",
                                                    class: if *basic_service_expectation.read() == "enabled" {
                                                        "bg-violet-500/20 text-violet-300"
                                                    } else {
                                                        "text-gray-400 hover:text-gray-200"
                                                    },
                                                    onclick: move |_| basic_service_expectation.set("enabled".to_string()),
                                                    "Enabled"
                                                }
                                                button {
                                                    class: "px-2 py-1 rounded text-[11px] transition-colors",
                                                    class: if *basic_service_expectation.read() == "disabled" {
                                                        "bg-violet-500/20 text-violet-300"
                                                    } else {
                                                        "text-gray-400 hover:text-gray-200"
                                                    },
                                                    onclick: move |_| basic_service_expectation.set("disabled".to_string()),
                                                    "Disabled"
                                                }
                                            }
                                        }

                                        if *basic_custom_builder.read() == BasicCustomBuilder::FirewallPortAllowed {
                                            div { class: "grid grid-cols-[1fr_auto] gap-2",
                                                input {
                                                    class: "w-full rounded-lg border px-3 py-2 text-xs cf-policy-modal-field focus:outline-none",
                                                    placeholder: "22",
                                                    value: "{basic_firewall_port}",
                                                    oninput: move |event| {
                                                        basic_firewall_port.set(event.value());
                                                        save_error.set(String::new());
                                                    },
                                                }
                                                div { class: "inline-flex rounded-md border border-gray-700 bg-gray-950/50 p-1",
                                                    button {
                                                        class: "px-2 py-1 rounded text-[11px] transition-colors",
                                                        class: if *basic_firewall_protocol.read() == "tcp" {
                                                            "bg-violet-500/20 text-violet-300"
                                                        } else {
                                                            "text-gray-400 hover:text-gray-200"
                                                        },
                                                        onclick: move |_| basic_firewall_protocol.set("tcp".to_string()),
                                                        "TCP"
                                                    }
                                                    button {
                                                        class: "px-2 py-1 rounded text-[11px] transition-colors",
                                                        class: if *basic_firewall_protocol.read() == "udp" {
                                                            "bg-violet-500/20 text-violet-300"
                                                        } else {
                                                            "text-gray-400 hover:text-gray-200"
                                                        },
                                                        onclick: move |_| basic_firewall_protocol.set("udp".to_string()),
                                                        "UDP"
                                                    }
                                                }
                                            }
                                            div { class: "inline-flex rounded-md border border-gray-700 bg-gray-950/50 p-1",
                                                button {
                                                    class: "px-2 py-1 rounded text-[11px] transition-colors",
                                                    class: if *basic_firewall_expectation.read() == "allowed" {
                                                        "bg-violet-500/20 text-violet-300"
                                                    } else {
                                                        "text-gray-400 hover:text-gray-200"
                                                    },
                                                    onclick: move |_| basic_firewall_expectation.set("allowed".to_string()),
                                                    "Allowed"
                                                }
                                                button {
                                                    class: "px-2 py-1 rounded text-[11px] transition-colors",
                                                    class: if *basic_firewall_expectation.read() == "denied" {
                                                        "bg-violet-500/20 text-violet-300"
                                                    } else {
                                                        "text-gray-400 hover:text-gray-200"
                                                    },
                                                    onclick: move |_| basic_firewall_expectation.set("denied".to_string()),
                                                    "Denied"
                                                }
                                            }
                                        }

                                        label { class: "text-xs text-violet-300/70 font-medium", "Result message" }
                                        input {
                                            class: "w-full rounded-lg border px-3 py-2 text-xs cf-policy-modal-field focus:outline-none",
                                            value: "{basic_rule_description}",
                                            placeholder: "Policy check failed",
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
                                            class: "w-full rounded-lg border px-3 py-2 text-xs cf-policy-modal-field focus:outline-none",
                                            placeholder: "git, vim, htop",
                                            value: "{basic_packages}",
                                            oninput: move |event| {
                                                basic_packages.set(event.value());
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
                                        class: "w-5 h-5 rounded-full border border-violet-400/60 bg-violet-500/10 text-xs font-semibold text-violet-200 hover:text-white hover:border-violet-300 inline-flex items-center justify-center",
                                        onclick: move |_| {
                                            let next = {
                                                let current = *show_strict_info.read();
                                                !current
                                            };
                                            show_strict_info.set(next);
                                        },
                                        "?"
                                    }
                                    if *show_strict_info.read() {
                                        span {
                                            class: "text-[10px] text-violet-200 bg-violet-500/15 border border-violet-400/40 rounded-full px-2 py-0.5 whitespace-nowrap",
                                            "false => fail eval, non-strict => record only"
                                        }
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
                                class: "rounded-lg border overflow-hidden flex-1 min-h-0 cf-policy-modal-editor-surface",
                                textarea {
                                    class: "w-full bg-transparent px-3 py-2 text-xs text-gray-100 font-mono focus:outline-none resize-none",
                                    style: "height: clamp(84px, 16vh, 140px);",
                                    rows: "6",
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

                    if let Some(message) = non_name_validation_error.clone() {
                        div {
                            class: "text-xs rounded px-3 py-2 cf-policy-modal-error",
                            "{message}"
                        }
                    }
                    if !save_error.read().is_empty() {
                        div {
                            class: "text-xs rounded px-3 py-2 cf-policy-modal-error",
                            "{save_error}"
                        }
                    }
                }

                // Footer
                div {
                    class: "flex justify-end items-center gap-3 pt-2 mt-2 border-t border-gray-800 shrink-0",
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
                            let custom_builder = *basic_custom_builder.read();
                            let expression = basic_expression.read().clone();
                            let service_name = basic_service_name.read().clone();
                            let service_expectation = basic_service_expectation.read().clone();
                            let firewall_port = basic_firewall_port.read().clone();
                            let firewall_protocol = basic_firewall_protocol.read().clone();
                            let firewall_expectation = basic_firewall_expectation.read().clone();
                            let rule_description = basic_rule_description.read().clone();
                            let packages_raw = basic_packages.read().clone();
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
                                            let (resolved_expression, default_message) = match custom_builder {
                                                BasicCustomBuilder::CustomExpression => (
                                                    expression.trim().to_string(),
                                                    "Custom rule failed".to_string(),
                                                ),
                                                BasicCustomBuilder::ServiceEnabled => {
                                                    let service = service_name.trim().to_string();
                                                    let expectation =
                                                        service_expectation.trim().to_lowercase();
                                                    let base_expr =
                                                        format!("config.services.{service}.enable");
                                                    (
                                                        if expectation == "disabled" {
                                                            format!("!{base_expr}")
                                                        } else {
                                                            base_expr
                                                        },
                                                        if expectation == "disabled" {
                                                            format!("Service must be disabled: {service}")
                                                        } else {
                                                            format!("Service must be enabled: {service}")
                                                        },
                                                    )
                                                }
                                                BasicCustomBuilder::FirewallPortAllowed => {
                                                    let port: u16 = firewall_port.trim().parse().unwrap_or(0);
                                                    let protocol = firewall_protocol.trim().to_lowercase();
                                                    let expectation = firewall_expectation.trim().to_lowercase();
                                                    let list_attr = if protocol == "udp" {
                                                        "allowedUDPPorts"
                                                    } else {
                                                        "allowedTCPPorts"
                                                    };
                                                    let base_expr = format!(
                                                        "builtins.elem {port} (config.networking.firewall.{list_attr} or [])"
                                                    );
                                                    let expr = if expectation == "denied" {
                                                        format!("!{base_expr}")
                                                    } else {
                                                        base_expr
                                                    };
                                                    let message = if expectation == "denied" {
                                                        format!("Firewall must deny {protocol}/{port}")
                                                    } else {
                                                        format!("Firewall must allow {protocol}/{port}")
                                                    };
                                                    (
                                                        expr,
                                                        message,
                                                    )
                                                }
                                            };

                                            let resolved_description = if rule_description.trim().is_empty() {
                                                default_message
                                            } else {
                                                rule_description.trim().to_string()
                                            };

                                            let cfg = serde_json::json!({
                                                "expression": resolved_expression,
                                                "description": resolved_description,
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
