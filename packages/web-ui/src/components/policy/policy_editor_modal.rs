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

const REQUIRE_CVE_CHECK_JSON_TEMPLATE: &str = r#"{
  "policy_type": "require_cve_check",
  "config": {
    "max_critical": 0,
    "max_high": null,
    "require_high_justification": false,
    "strict": true,
    "when_no_scan": "block"
  }
}"#;

const MULTI_RULE_JSON_TEMPLATE: &str = r#"{
  "policy_type": "custom_check",
  "config": {
    "rules": [
      {
        "expression": "config.services.crystal-forge.enable or false",
        "description": "Crystal Forge agent is enabled",
        "field_name": "cfAgentEnabled",
        "strict": true
      },
      {
        "expression": "builtins.elem \"git\" (builtins.map (p: p.pname or \"\") config.environment.systemPackages)",
        "description": "git is installed",
        "field_name": "gitInstalled",
        "strict": true
      }
    ],
    "mode": "all",
    "strict": true
  }
}"#;

#[derive(Clone, Copy, PartialEq, Eq)]
enum BasicPolicyKind {
    CustomCheck,
    RequirePackages,
    RequireCveCheck,
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
        "Edit custom policy"
    } else {
        "New custom policy"
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
        } else if initial_policy_type == "require_cve_check" {
            BasicPolicyKind::RequireCveCheck
        } else {
            BasicPolicyKind::CustomCheck
        }
    });
    let mut cve_max_critical = use_signal(|| {
        initial_config
            .get("max_critical")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            .to_string()
    });
    let mut cve_max_high = use_signal(|| {
        initial_config
            .get("max_high")
            .and_then(|v| v.as_u64())
            .map(|v| v.to_string())
            .unwrap_or_default()
    });
    let mut cve_require_justification = use_signal(|| {
        initial_config
            .get("require_high_justification")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    });
    let mut cve_when_no_scan = use_signal(|| {
        initial_config
            .get("when_no_scan")
            .and_then(|v| v.as_str())
            .unwrap_or("block")
            .to_string()
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
    let mut design_category = use_signal(|| {
        if initial_policy_type == "require_cve_check" {
            "scanning".to_string()
        } else if initial_policy_type == "require_packages" || initial_policy_type == "custom_check"
        {
            "security".to_string()
        } else {
            "deployment".to_string()
        }
    });
    let mut design_severity = use_signal(|| "medium".to_string());
    let mut design_rationale = use_signal(String::new);
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
                BasicPolicyKind::RequireCveCheck => {
                    let max_critical_str = cve_max_critical.read().trim().to_string();
                    if !max_critical_str.is_empty() && max_critical_str.parse::<u32>().is_err() {
                        Some("Max critical CVEs must be a non-negative integer".to_string())
                    } else {
                        let max_high_str = cve_max_high.read().trim().to_string();
                        if !max_high_str.is_empty() && max_high_str.parse::<u32>().is_err() {
                            Some(
                                "Max high CVEs must be a non-negative integer or blank".to_string(),
                            )
                        } else {
                            None
                        }
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
                class: "modal-backdrop cf-modal-overlay-z50",
                onclick: move |_| on_close.call(()),

                div {
                    class: "modal cf-policy-modal-panel",
                    style: "width:min(680px,96vw);max-height:92vh;",
                    onclick: |evt| evt.stop_propagation(),

                    // Header
                    div {
                        class: "modal-head",
                        div {
                            class: "flex items-center justify-between gap-3",
                            div {
                                h2 {
                                    svg {
                                        width: "14", height: "14", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:6px;vertical-align:text-bottom;",
                                        if is_editing {
                                            path { d: "M12 15.5A3.5 3.5 0 1 0 12 8a3.5 3.5 0 0 0 0 7.5Z" }
                                            path { d: "M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06A1.65 1.65 0 0 0 15 19.4a1.65 1.65 0 0 0-1 .6l-.09.09a2 2 0 0 1-3.82-1.18l.01-.1A1.65 1.65 0 0 0 9 17.4a1.65 1.65 0 0 0-1.82-.33l-.08.03a2 2 0 0 1-2.18-3.25l.08-.05A1.65 1.65 0 0 0 5.6 12a1.65 1.65 0 0 0-.6-1.4l-.08-.05A2 2 0 0 1 7.1 7.3l.08.03A1.65 1.65 0 0 0 9 6.6a1.65 1.65 0 0 0 .33-1.82l-.01-.1a2 2 0 0 1 3.82-1.18l.09.09a1.65 1.65 0 0 0 1 .6A1.65 1.65 0 0 0 16.9 4l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9c.38.28.6.73.6 1.2s-.22.92-.6 1.2Z" }
                                        } else {
                                            path { d: "M12 5v14M5 12h14" }
                                        }
                                    }
                                    "{title}"
                                }
                                p {
                                    if is_editing {
                                        "Update the rules and rationale."
                                    } else {
                                        "Compose a policy from gate rules. Systems can be assigned this policy from their edit dialog."
                                    }
                                }
                            }
                            button {
                                class: "btn-icon focus-ring",
                                onclick: move |_| on_close.call(()),
                                svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "M18 6 6 18M6 6l12 12" }
                                }
                            }
                        }
                        div { style: "margin-top:10px;display:flex;justify-content:flex-end;",
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
                                                BasicPolicyKind::RequireCveCheck => {
                                                    let max_critical: u32 = cve_max_critical
                                                        .read()
                                                        .trim()
                                                        .parse()
                                                        .unwrap_or(0);
                                                    let max_high: serde_json::Value = cve_max_high
                                                        .read()
                                                        .trim()
                                                        .parse::<u32>()
                                                        .map(|v| serde_json::json!(v))
                                                        .unwrap_or(serde_json::Value::Null);
                                                    let require_justification =
                                                        *cve_require_justification.read();
                                                    let when_no_scan =
                                                        cve_when_no_scan.read().clone();
                                                    (
                                                        "require_cve_check".to_string(),
                                                        serde_json::json!({
                                                            "max_critical": max_critical,
                                                            "max_high": max_high,
                                                            "require_high_justification": require_justification,
                                                            "strict": true,
                                                            "when_no_scan": when_no_scan,
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
                        }
                    }

                    // Form content
                    div {
                        class: "modal-body",
                        style: "overflow-y:auto;",

                        // Left column - metadata
                        div {
                            class: "space-y-3 min-h-0",
                            div {
                                class: "space-y-2",
                                label { "Name" }
                                input {
                                    class: if name_missing_error {
                                        "input focus-ring mono cf-policy-modal-field-error"
                                    } else {
                                        "input focus-ring mono"
                                    },
                                    placeholder: "e.g. canary-25",
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
                                label { "Description" }
                                input {
                                    class: "input focus-ring",
                                    placeholder: "One-line summary shown in the registry",
                                    value: "{edit_description}",
                                    oninput: move |event| {
                                        edit_description.set(event.value());
                                        save_error.set(String::new());
                                    },
                                }
                            }
                            div { class: "field",
                                label { "Category" }
                                div { style: "display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:8px;",
                                    for (id, label, blurb, color) in [
                                        ("deployment", "Deployment gates", "Criteria a system must satisfy before deploy.", "#a78bfa"),
                                        ("security", "Security baseline", "Host configuration and hardening assertions.", "#60a5fa"),
                                        ("scanning", "Vulnerability gates", "CVE and scan-result blockers.", "#fbbf24"),
                                        ("rollout", "Rollout controls", "Approval, timing, and canary constraints.", "#34d399"),
                                    ] {
                                        button {
                                            class: "focus-ring",
                                            style: if design_category.read().as_str() == id {
                                                "display:flex;align-items:flex-start;gap:9px;text-align:left;padding:9px 11px;border-radius:9px;cursor:pointer;background:color-mix(in oklab, {color} 12%, transparent);border:1px solid color-mix(in oklab, {color} 55%, transparent);"
                                            } else {
                                                "display:flex;align-items:flex-start;gap:9px;text-align:left;padding:9px 11px;border-radius:9px;cursor:pointer;background:var(--cf-subtle-bg);border:1px solid var(--cf-divider);"
                                            },
                                            onclick: move |_| {
                                                design_category.set(id.to_string());
                                                if id == "scanning" {
                                                    basic_kind.set(BasicPolicyKind::RequireCveCheck);
                                                } else if id == "security" {
                                                    basic_kind.set(BasicPolicyKind::CustomCheck);
                                                } else if id == "rollout" {
                                                    advanced_mode.set(true);
                                                    edit_format.set(PolicyFormat::Json);
                                                    edit_body.set(MULTI_RULE_JSON_TEMPLATE.to_string());
                                                }
                                                save_error.set(String::new());
                                            },
                                            span { style: "flex-shrink:0;width:24px;height:24px;border-radius:6px;display:grid;place-items:center;background:color-mix(in oklab, {color} 16%, transparent);color:{color};",
                                                svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                                    path { d: "M9 12l2 2 4-4" }
                                                    path { d: "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0z" }
                                                }
                                            }
                                            span { style: "min-width:0;",
                                                span { style: if design_category.read().as_str() == id { "display:block;font-size:12px;font-weight:600;color:{color};" } else { "display:block;font-size:12px;font-weight:600;color:var(--cf-text-primary);" }, "{label}" }
                                                span { style: "display:block;font-size:10.5px;color:var(--cf-text-muted);line-height:1.35;margin-top:2px;", "{blurb}" }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "field",
                                label { "Severity" }
                                div { class: "seg seg-sev", style: "width:fit-content;",
                                    for (value, label, color) in [
                                        ("high", "High (CAT I)", "#f87171"),
                                        ("medium", "Medium (CAT II)", "#fbbf24"),
                                        ("low", "Low (CAT III)", "#60a5fa"),
                                    ] {
                                        button {
                                            class: if design_severity.read().as_str() == value { "active" } else { "" },
                                            style: if design_severity.read().as_str() == value {
                                                "color:{color};background:color-mix(in oklab, {color} 16%, transparent);box-shadow:inset 0 0 0 1px color-mix(in oklab, {color} 45%, transparent);"
                                            } else {
                                                "color:var(--cf-text-secondary);"
                                            },
                                            onclick: move |_| design_severity.set(value.to_string()),
                                            span { style: "display:inline-flex;align-items:center;gap:6px;",
                                                span { style: "width:7px;height:7px;border-radius:50%;background:{color};" }
                                                "{label}"
                                            }
                                        }
                                    }
                                }
                                div { class: "help", "Drives how failures of this control are weighted in compliance scoring and evidence reports." }
                            }
                            div { class: "field",
                                label { "Rationale" }
                                textarea {
                                    class: "input focus-ring",
                                    rows: "2",
                                    placeholder: "Why this policy exists — shown in detail view",
                                    style: "resize:vertical;",
                                    value: "{design_rationale}",
                                    oninput: move |event| design_rationale.set(event.value()),
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
                            div { style: "margin-top:6px;",
                                div { style: "display:flex;justify-content:space-between;align-items:baseline;margin-bottom:8px;",
                                    label { style: "font-size:12px;font-weight:600;color:var(--cf-text-primary);", "Assertions & gate rules" }
                                    span { style: "font-size:11px;color:var(--cf-text-muted);", "All must hold — each compiles to a policy check." }
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
                                          button {
                                              class: "px-3 py-1.5 rounded-md text-xs border border-amber-700/60 text-amber-300/80 hover:bg-amber-900/20",
                                             onclick: move |_| {
                                                 edit_format.set(PolicyFormat::Json);
                                                 edit_body.set(REQUIRE_CVE_CHECK_JSON_TEMPLATE.to_string());
                                                 save_error.set(String::new());
                                             },
                                              "CVE gate"
                                          }
                                          button {
                                              class: "px-3 py-1.5 rounded-md text-xs border border-gray-700 text-gray-300 hover:bg-gray-800",
                                              onclick: move |_| {
                                                  edit_format.set(PolicyFormat::Json);
                                                  edit_body.set(MULTI_RULE_JSON_TEMPLATE.to_string());
                                                  save_error.set(String::new());
                                              },
                                              "Multi-rule"
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
                                             button {
                                                 class: "px-3 py-1.5 rounded-md text-xs border transition-colors",
                                                 class: if *basic_kind.read() == BasicPolicyKind::RequireCveCheck {
                                                     "bg-amber-500/20 border-amber-500 text-amber-300"
                                                 } else {
                                                     "border-gray-700 text-gray-300 hover:bg-gray-800"
                                                 },
                                                 onclick: move |_| {
                                                     basic_kind.set(BasicPolicyKind::RequireCveCheck);
                                                     save_error.set(String::new());
                                                 },
                                                 "CVE gate"
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

                                    if *basic_kind.read() == BasicPolicyKind::RequireCveCheck {
                                        div { class: "space-y-3",
                                            p {
                                                class: "text-[11px] text-amber-300/70 bg-amber-900/20 border border-amber-700/40 rounded-md px-2 py-1.5",
                                                "CVE gate runs after build-complete, before deployment. Requires vulnix scans to be active."
                                            }
                                            div { class: "space-y-1",
                                                label { class: "text-xs text-amber-300/70 font-medium", "Max critical CVEs" }
                                                input {
                                                    r#type: "number",
                                                    min: "0",
                                                    class: "w-full rounded-lg border px-3 py-2 text-xs cf-policy-modal-field focus:outline-none",
                                                    placeholder: "0",
                                                    value: "{cve_max_critical}",
                                                    oninput: move |event| {
                                                        cve_max_critical.set(event.value());
                                                        save_error.set(String::new());
                                                    },
                                                }
                                                p { class: "text-[10px] text-gray-500", "Deployment blocked if critical CVE count exceeds this." }
                                            }
                                            div { class: "space-y-1",
                                                label { class: "text-xs text-amber-300/70 font-medium", "Max high CVEs (leave blank = no limit)" }
                                                input {
                                                    r#type: "number",
                                                    min: "0",
                                                    class: "w-full rounded-lg border px-3 py-2 text-xs cf-policy-modal-field focus:outline-none",
                                                    placeholder: "blank = no limit",
                                                    value: "{cve_max_high}",
                                                    oninput: move |event| {
                                                        cve_max_high.set(event.value());
                                                        save_error.set(String::new());
                                                    },
                                                }
                                            }
                                            div { class: "flex items-center gap-2",
                                                input {
                                                    r#type: "checkbox",
                                                    id: "cve-require-justification",
                                                    checked: *cve_require_justification.read(),
                                                    onchange: move |_| {
                                                        let next = !*cve_require_justification.read();
                                                        cve_require_justification.set(next);
                                                    }
                                                }
                                                label {
                                                    r#for: "cve-require-justification",
                                                    class: "text-xs text-gray-300",
                                                    "Require whitelist justification for high CVEs"
                                                }
                                            }
                                            div { class: "space-y-1",
                                                label { class: "text-xs text-amber-300/70 font-medium", "When no scan exists" }
                                                div { class: "inline-flex rounded-md border border-gray-700 bg-gray-950/50 p-1",
                                                    button {
                                                        class: "px-2 py-1 rounded text-[11px] transition-colors",
                                                        class: if *cve_when_no_scan.read() == "block" {
                                                            "bg-amber-500/20 text-amber-300"
                                                        } else {
                                                            "text-gray-400 hover:text-gray-200"
                                                        },
                                                        onclick: move |_| cve_when_no_scan.set("block".to_string()),
                                                        "Block"
                                                    }
                                                    button {
                                                        class: "px-2 py-1 rounded text-[11px] transition-colors",
                                                        class: if *cve_when_no_scan.read() == "skip" {
                                                            "bg-green-500/20 text-green-300"
                                                        } else {
                                                            "text-gray-400 hover:text-gray-200"
                                                        },
                                                        onclick: move |_| cve_when_no_scan.set("skip".to_string()),
                                                        "Skip"
                                                    }
                                                }
                                                p { class: "text-[10px] text-gray-500",
                                                    if *cve_when_no_scan.read() == "block" {
                                                        "Deployment blocked if no scan has run."
                                                    } else {
                                                        "Deployment allowed if no scan has run yet."
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    if *basic_kind.read() != BasicPolicyKind::RequireCveCheck {
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
                                } // closes div { class: "space-y-2" } (templates/type picker)

                            if !*advanced_mode.read() {
                                div {
                                    class: "rounded-lg border border-gray-700 bg-gray-950/40 p-3 space-y-1",
                                    p { class: "text-xs text-gray-300", "Basic mode keeps common policy creation compact." }
                                    p { class: "text-xs {theme::text::MUTED}", "Use Advanced for CVE/multi-rule JSON or TOML payloads." }
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
                        class: "modal-foot",
                        button {
                            class: "btn btn-ghost focus-ring",
                            onclick: move |_| on_close.call(()),
                            "Cancel"
                        }
                        button {
                            class: "btn btn-primary focus-ring",
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
                                 let cve_max_critical_raw = cve_max_critical.read().clone();
                                 let cve_max_high_raw = cve_max_high.read().clone();
                                 let cve_require_high_justification =
                                     *cve_require_justification.read();
                                 let cve_when_no_scan_value = cve_when_no_scan.read().clone();
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
                                             BasicPolicyKind::RequireCveCheck => {
                                                 let max_critical = cve_max_critical_raw
                                                     .trim()
                                                     .parse::<u32>()
                                                     .unwrap_or(0);
                                                 let max_high_value = cve_max_high_raw.trim();
                                                 let max_high_json = if max_high_value.is_empty() {
                                                     serde_json::Value::Null
                                                 } else {
                                                     serde_json::json!(
                                                         max_high_value.parse::<u32>().unwrap_or(0)
                                                     )
                                                 };
                                                 let when_no_scan = if cve_when_no_scan_value.trim() == "skip" {
                                                     "skip"
                                                 } else {
                                                     "block"
                                                 };
                                                 let cfg = serde_json::json!({
                                                     "max_critical": max_critical,
                                                     "max_high": max_high_json,
                                                     "require_high_justification": cve_require_high_justification,
                                                     "strict": true,
                                                     "when_no_scan": when_no_scan,
                                                 });
                                                 ("require_cve_check".to_string(), cfg)
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
}
