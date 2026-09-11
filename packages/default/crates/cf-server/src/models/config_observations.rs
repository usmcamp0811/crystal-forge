//! Defines the typed contract for scoped Config Explorer observations.
//!
//! These observations are non-authoritative and cannot provide deployment or
//! policy evidence. Every identity uses structured path components.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

/// Current scoped observation schema version.
pub const CONFIG_OBSERVATION_SCHEMA_VERSION: i32 = 1;
/// Maximum structured option path depth.
pub const MAX_CONFIG_OBSERVATION_PATH_DEPTH: usize = 16;
/// Maximum Unicode scalar count in one path component.
pub const MAX_CONFIG_OBSERVATION_COMPONENT_CHARS: usize = 256;
const MAX_CONFIG_OBSERVATION_PATH_JSON_BYTES: usize = 8192;
/// Maximum children or definitions returned by one scoped observation.
pub const MAX_CONFIG_OBSERVATION_ITEMS: usize = 512;
/// Maximum configured identities retained in one batched index payload.
pub const MAX_CONFIGURED_OBSERVATION_ITEMS: usize = 512;
/// Maximum diagnostics retained for each configured-index diagnostic class.
pub const MAX_CONFIG_OBSERVATION_DIAGNOSTICS: usize = 128;

/// Selects one trusted Config Explorer operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigObservationKind {
    /// Lists only immediate top-level option and prefix names.
    Root,
    /// Lists only immediate children below one exact prefix.
    Prefix,
    /// Reads basic metadata and a safe value for one exact option.
    Option,
    /// Reads definition provenance for one exact option without definition values.
    Provenance,
    /// Builds the asynchronous index of explicitly configured option identities.
    ConfiguredIndex,
}

impl ConfigObservationKind {
    /// Returns the stable persistence and Nix operation name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Prefix => "prefix",
            Self::Option => "option",
            Self::Provenance => "provenance",
            Self::ConfiguredIndex => "configured_index",
        }
    }

    /// Parses a trusted persistence value.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is not a supported operation.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "root" => Ok(Self::Root),
            "prefix" => Ok(Self::Prefix),
            "option" => Ok(Self::Option),
            "provenance" => Ok(Self::Provenance),
            "configured_index" => Ok(Self::ConfiguredIndex),
            _ => bail!("unknown Config observation kind"),
        }
    }

    /// Returns the server-owned queue priority.
    pub fn priority(self) -> i16 {
        match self {
            Self::ConfiguredIndex => 100,
            Self::Root | Self::Prefix | Self::Option | Self::Provenance => 10,
        }
    }
}

/// Validates a structured option path for one operation.
///
/// Root and configured-index operations require an empty path. All other
/// operations require at least one component.
///
/// # Errors
///
/// Returns an error when depth, component length, emptiness, or encoded size
/// exceeds the persistence contract.
pub fn validate_config_observation_path(
    kind: ConfigObservationKind,
    path: &[String],
) -> Result<()> {
    let requires_empty = matches!(
        kind,
        ConfigObservationKind::Root | ConfigObservationKind::ConfiguredIndex
    );
    if requires_empty != path.is_empty() || !config_observation_path_is_bounded(path)? {
        bail!("invalid structured Config observation path");
    }
    Ok(())
}

fn config_observation_path_is_bounded(path: &[String]) -> Result<bool> {
    Ok(path.len() <= MAX_CONFIG_OBSERVATION_PATH_DEPTH
        && !path.iter().any(|component| {
            component.is_empty()
                || component.chars().count() > MAX_CONFIG_OBSERVATION_COMPONENT_CHARS
        })
        && serde_json::to_vec(path)?.len() <= MAX_CONFIG_OBSERVATION_PATH_JSON_BYTES)
}

fn exact_fields(object: &Map<String, Value>, expected: &[&str]) -> bool {
    object.len() == expected.len() && expected.iter().all(|field| object.contains_key(*field))
}

fn payload_path(object: &Map<String, Value>, field: &str) -> Result<Vec<String>> {
    serde_json::from_value(
        object
            .get(field)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Config observation payload path is missing"))?,
    )
    .context("Config observation payload path is malformed")
}

fn valid_key(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|key| key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn valid_bounded_diagnostic(value: &Value, with_path: bool) -> Result<bool> {
    let Some(object) = value.as_object() else {
        return Ok(false);
    };
    let expected = if with_path {
        &["path_components", "code", "message"][..]
    } else {
        &["key", "code", "message"][..]
    };
    if !exact_fields(object, expected)
        || !object
            .get("code")
            .and_then(Value::as_str)
            .is_some_and(|value| value.len() <= 128)
        || !object
            .get("message")
            .and_then(Value::as_str)
            .is_some_and(|value| value.len() <= 512)
    {
        return Ok(false);
    }
    if with_path {
        config_observation_path_is_bounded(&payload_path(object, "path_components")?)
    } else {
        Ok(valid_key(object.get("key")))
    }
}

fn valid_optional_bounded_string(value: Option<&Value>, max_bytes: usize) -> bool {
    value.is_some_and(|value| {
        value.is_null() || value.as_str().is_some_and(|value| value.len() <= max_bytes)
    })
}

fn valid_encoded_value(value: &Value, depth: usize) -> bool {
    if depth > 17 {
        return false;
    }
    let Some(object) = value.as_object() else {
        return false;
    };
    if !exact_fields(object, &["kind", "value"]) {
        return false;
    }
    let encoded = &object["value"];
    match object.get("kind").and_then(Value::as_str) {
        Some("scalar") => match encoded {
            Value::Null | Value::Bool(_) | Value::Number(_) => true,
            Value::String(value) => value.len() <= 1024 * 1024,
            _ => false,
        },
        Some("package") => encoded.as_object().is_some_and(|package| {
            exact_fields(package, &["name", "pname", "version", "output_path"])
                && ["name", "pname", "version", "output_path"]
                    .into_iter()
                    .all(|field| valid_optional_bounded_string(package.get(field), 4096))
        }),
        Some("list") => encoded.as_array().is_some_and(|items| {
            items.len() <= 256
                && items
                    .iter()
                    .all(|item| valid_encoded_value(item, depth + 1))
        }),
        Some("attribute_set" | "submodule") => encoded.as_object().is_some_and(|fields| {
            fields.len() <= 256
                && fields.iter().all(|(name, value)| {
                    name.chars().count() <= MAX_CONFIG_OBSERVATION_COMPONENT_CHARS
                        && (value.as_str()
                            == Some(crate::security::snapshot_redaction::REDACTED_VALUE)
                            || valid_encoded_value(value, depth + 1))
                })
        }),
        Some("opaque") => encoded.as_object().is_some_and(|opaque| {
            exact_fields(opaque, &["type_name"])
                && opaque
                    .get("type_name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.len() <= 128)
        }),
        _ => false,
    }
}

/// Validates one redacted observation payload against its exact request identity.
///
/// The closed shape prevents evaluator output from adding browser-controlled
/// source, provenance, or execution fields. Array and text bounds keep every
/// accepted payload within the single-write cache contract.
///
/// # Errors
///
/// Returns an error when the payload shape, primitive types, bounds, operation,
/// or path identity does not match the request.
pub(crate) fn validate_config_observation_payload(
    kind: ConfigObservationKind,
    path: &[String],
    payload: &Value,
) -> Result<()> {
    validate_config_observation_path(kind, path)?;
    let object = payload
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Config observation payload must be an object"))?;
    if object.get("kind").and_then(Value::as_str) != Some(kind.as_str())
        || payload_path(object, "path_components")? != path
    {
        bail!("Config observation payload identity does not match its request");
    }

    let valid = match kind {
        ConfigObservationKind::Root | ConfigObservationKind::Prefix => {
            let children = object.get("children").and_then(Value::as_array);
            let total = object.get("total_children").and_then(Value::as_u64);
            let truncated = object.get("children_truncated").and_then(Value::as_bool);
            exact_fields(
                object,
                &[
                    "kind",
                    "path_components",
                    "children",
                    "children_truncated",
                    "total_children",
                ],
            ) && children.is_some_and(|children| {
                children.len() <= MAX_CONFIG_OBSERVATION_ITEMS
                    && children.iter().all(|child| {
                        let Some(child) = child.as_object() else {
                            return false;
                        };
                        let Ok(child_path) = payload_path(child, "path_components") else {
                            return false;
                        };
                        exact_fields(child, &["path_components", "key", "kind"])
                            && child_path.len() == path.len() + 1
                            && child_path.starts_with(path)
                            && config_observation_path_is_bounded(&child_path).unwrap_or(false)
                            && valid_key(child.get("key"))
                            && matches!(
                                child.get("kind").and_then(Value::as_str),
                                Some("option" | "prefix" | "unavailable")
                            )
                    })
            }) && total.is_some_and(|total| {
                let count = children.map_or(0, Vec::len) as u64;
                truncated.is_some_and(|truncated| {
                    if truncated {
                        total > count
                    } else {
                        total == count
                    }
                })
            })
        }
        ConfigObservationKind::Option => {
            exact_fields(
                object,
                &[
                    "kind",
                    "path_components",
                    "key",
                    "declared_type",
                    "is_defined",
                    "highest_prio",
                    "value",
                ],
            ) && valid_key(object.get("key"))
                && object.get("declared_type").is_some_and(|value| {
                    value.is_null()
                        || value
                            .as_str()
                            .is_some_and(|declared_type| declared_type.len() <= 4096)
                })
                && object.get("is_defined").is_some_and(Value::is_boolean)
                && object
                    .get("highest_prio")
                    .is_some_and(|value| value.is_null() || value.as_i64().is_some())
                && object
                    .get("value")
                    .is_some_and(|value| valid_encoded_value(value, 0))
        }
        ConfigObservationKind::Provenance => {
            let definitions = object.get("definitions").and_then(Value::as_array);
            let total = object.get("total_definitions").and_then(Value::as_u64);
            let truncated = object.get("definitions_truncated").and_then(Value::as_bool);
            exact_fields(
                object,
                &[
                    "kind",
                    "path_components",
                    "key",
                    "definitions",
                    "definitions_truncated",
                    "total_definitions",
                ],
            ) && valid_key(object.get("key"))
                && definitions.is_some_and(|definitions| {
                    definitions.len() <= MAX_CONFIG_OBSERVATION_ITEMS
                        && definitions.iter().all(|definition| {
                            definition.as_object().is_some_and(|definition| {
                                exact_fields(definition, &["source_path", "priority"])
                                    && valid_optional_bounded_string(
                                        definition.get("source_path"),
                                        4096,
                                    )
                                    && definition.get("priority").is_some_and(|value| {
                                        value.is_null() || value.as_i64().is_some()
                                    })
                            })
                        })
                })
                && total.is_some_and(|total| {
                    let count = definitions.map_or(0, Vec::len) as u64;
                    truncated.is_some_and(|truncated| {
                        if truncated {
                            total > count
                        } else {
                            total == count
                        }
                    })
                })
        }
        ConfigObservationKind::ConfiguredIndex => {
            let configured = object.get("configured").and_then(Value::as_array);
            let total_configured = object.get("total_configured").and_then(Value::as_u64);
            let configured_truncated = object.get("configured_truncated").and_then(Value::as_bool);
            let traversal_diagnostics = object.get("diagnostics").and_then(Value::as_array);
            let classifier_diagnostics = object
                .get("classifier_diagnostics")
                .and_then(Value::as_array);
            exact_fields(
                object,
                &[
                    "kind",
                    "path_components",
                    "total_traversed",
                    "diagnostics",
                    "diagnostics_truncated",
                    "configured",
                    "total_configured",
                    "configured_truncated",
                    "classifier_diagnostics",
                    "classifier_diagnostics_truncated",
                ],
            ) && object
                .get("total_traversed")
                .and_then(Value::as_u64)
                .is_some()
                && total_configured.is_some_and(|total| {
                    let count = configured.map_or(0, Vec::len) as u64;
                    configured_truncated.is_some_and(|truncated| {
                        if truncated {
                            total > count
                        } else {
                            total == count
                        }
                    })
                })
                && object
                    .get("diagnostics_truncated")
                    .is_some_and(Value::is_boolean)
                && object
                    .get("configured_truncated")
                    .is_some_and(Value::is_boolean)
                && object
                    .get("classifier_diagnostics_truncated")
                    .is_some_and(Value::is_boolean)
                && configured.is_some_and(|configured| {
                    configured.len() <= MAX_CONFIGURED_OBSERVATION_ITEMS
                        && configured.iter().all(|entry| {
                            entry.as_object().is_some_and(|entry| {
                                let Ok(entry_path) = payload_path(entry, "path_components") else {
                                    return false;
                                };
                                exact_fields(entry, &["path_components", "key"])
                                    && config_observation_path_is_bounded(&entry_path)
                                        .unwrap_or(false)
                                    && !entry_path.is_empty()
                                    && valid_key(entry.get("key"))
                            })
                        })
                })
                && traversal_diagnostics.is_some_and(|diagnostics| {
                    diagnostics.len() <= MAX_CONFIG_OBSERVATION_DIAGNOSTICS
                        && diagnostics
                            .iter()
                            .all(|value| valid_bounded_diagnostic(value, true).unwrap_or(false))
                })
                && classifier_diagnostics.is_some_and(|diagnostics| {
                    diagnostics.len() <= MAX_CONFIG_OBSERVATION_DIAGNOSTICS
                        && diagnostics
                            .iter()
                            .all(|value| valid_bounded_diagnostic(value, false).unwrap_or(false))
                })
        }
    };
    if !valid {
        bail!("Config observation payload violates the operation contract");
    }
    Ok(())
}

/// Requests creation or reuse of one exact scoped observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateConfigObservationRequest {
    /// Trusted operation selected from a closed enum.
    pub kind: ConfigObservationKind,
    /// Exact option path components. No Nix source is accepted.
    pub path_components: Vec<String>,
}

/// Describes the durable scoped request lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigObservationLifecycle {
    /// The request is eligible for a future worker pass.
    Queued,
    /// The worker found the request but authoritative Nix capacity is busy.
    WaitingForCapacity,
    /// Capacity is held and an execution ID has a live heartbeat.
    Running,
    /// An immutable observation is available.
    Succeeded,
    /// The bounded execution failed.
    Failed,
}

impl ConfigObservationLifecycle {
    /// Parses a trusted persistence value.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown lifecycle value.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "waiting_for_capacity" => Ok(Self::WaitingForCapacity),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            _ => bail!("unknown Config observation lifecycle"),
        }
    }
}

/// Reports one exact request without exposing evaluator arguments or credentials.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigObservationRequestResponse {
    /// Durable request identity.
    pub request_id: Uuid,
    /// Full immutable commit SHA.
    pub revision: String,
    /// Effective NixOS configuration name.
    pub configuration_name: String,
    /// Scoped operation.
    pub kind: ConfigObservationKind,
    /// Exact validated path components.
    pub path_components: Vec<String>,
    /// Current lifecycle.
    pub lifecycle: ConfigObservationLifecycle,
    /// Immutable observation identity after success.
    pub observation_id: Option<Uuid>,
    /// Stable bounded failure message after failure.
    pub error: Option<String>,
    /// Number of executions that acquired capacity.
    pub attempts: i32,
    /// Last running heartbeat. Waiting requests never have a heartbeat.
    pub heartbeat_at: Option<DateTime<Utc>>,
    /// True when the POST reused a cache entry or active request.
    #[serde(default)]
    pub reused: bool,
}

/// Returns one immutable scoped observation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigObservationResponse {
    /// Immutable observation identity.
    pub observation_id: Uuid,
    /// Full immutable commit SHA.
    pub revision: String,
    /// Effective NixOS configuration name.
    pub configuration_name: String,
    /// Observation schema version.
    pub schema_version: i32,
    /// Scoped operation.
    pub kind: ConfigObservationKind,
    /// Exact structured path components.
    pub path_components: Vec<String>,
    /// Redacted bounded observation payload.
    pub payload: Value,
    /// Observation creation time.
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_paths_reject_bounds_but_accept_injection_shaped_components() {
        for component in [
            "services.\"; builtins.abort \"x",
            "${builtins.readFile /etc/passwd}",
            "a.b",
        ] {
            assert!(
                validate_config_observation_path(
                    ConfigObservationKind::Prefix,
                    &[component.to_string()]
                )
                .is_ok()
            );
        }
        assert!(validate_config_observation_path(ConfigObservationKind::Root, &[]).is_ok());
        assert!(
            validate_config_observation_path(ConfigObservationKind::Root, &["x".into()]).is_err()
        );
        assert!(validate_config_observation_path(ConfigObservationKind::Option, &[]).is_err());
        assert!(
            validate_config_observation_path(
                ConfigObservationKind::Option,
                &vec!["x".into(); MAX_CONFIG_OBSERVATION_PATH_DEPTH + 1]
            )
            .is_err()
        );
        assert!(
            validate_config_observation_path(
                ConfigObservationKind::Option,
                &["x".repeat(MAX_CONFIG_OBSERVATION_COMPONENT_CHARS + 1)]
            )
            .is_err()
        );
    }

    #[test]
    fn request_rejects_all_non_contract_evaluator_fields() {
        for field in ["expression", "apply", "source", "nix_args"] {
            let mut request = serde_json::json!({
                "kind": "root",
                "path_components": []
            });
            request.as_object_mut().unwrap().insert(
                field.to_string(),
                Value::String("builtins.abort \"browser source executed\"".to_string()),
            );
            assert!(serde_json::from_value::<CreateConfigObservationRequest>(request).is_err());
        }
    }

    #[test]
    fn every_operation_accepts_its_exact_bounded_payload_contract() {
        let key = "a".repeat(64);
        let cases = [
            (
                ConfigObservationKind::Root,
                Vec::new(),
                serde_json::json!({
                    "kind": "root",
                    "path_components": [],
                    "children": [{"path_components": ["services"], "key": key, "kind": "prefix"}],
                    "children_truncated": false,
                    "total_children": 1
                }),
            ),
            (
                ConfigObservationKind::Prefix,
                vec!["services".to_string()],
                serde_json::json!({
                    "kind": "prefix",
                    "path_components": ["services"],
                    "children": [{"path_components": ["services", "nginx"], "key": key, "kind": "option"}],
                    "children_truncated": false,
                    "total_children": 1
                }),
            ),
            (
                ConfigObservationKind::Option,
                vec!["services".to_string(), "nginx".to_string()],
                serde_json::json!({
                    "kind": "option",
                    "path_components": ["services", "nginx"],
                    "key": key,
                    "declared_type": "boolean",
                    "is_defined": true,
                    "highest_prio": 100,
                    "value": {"kind": "scalar", "value": true}
                }),
            ),
            (
                ConfigObservationKind::Provenance,
                vec!["services".to_string(), "nginx".to_string()],
                serde_json::json!({
                    "kind": "provenance",
                    "path_components": ["services", "nginx"],
                    "key": key,
                    "definitions": [{"source_path": "/flake/module.nix", "priority": 100}],
                    "definitions_truncated": false,
                    "total_definitions": 1
                }),
            ),
            (
                ConfigObservationKind::ConfiguredIndex,
                Vec::new(),
                serde_json::json!({
                    "kind": "configured_index",
                    "path_components": [],
                    "total_traversed": 1,
                    "diagnostics": [],
                    "diagnostics_truncated": false,
                    "configured": [{"path_components": ["services", "nginx"], "key": key}],
                    "total_configured": 1,
                    "configured_truncated": false,
                    "classifier_diagnostics": [],
                    "classifier_diagnostics_truncated": false
                }),
            ),
        ];

        for (kind, path, payload) in cases {
            validate_config_observation_payload(kind, &path, &payload)
                .unwrap_or_else(|error| panic!("{kind:?} payload should validate: {error:#}"));
        }
    }

    #[test]
    fn configured_payload_rejects_values_provenance_and_nonconfigured_entries() {
        let valid = serde_json::json!({
            "kind": "configured_index",
            "path_components": [],
            "total_traversed": 1,
            "diagnostics": [],
            "diagnostics_truncated": false,
            "configured": [{"path_components": ["services", "ok"], "key": "a".repeat(64)}],
            "total_configured": 1,
            "configured_truncated": false,
            "classifier_diagnostics": [],
            "classifier_diagnostics_truncated": false
        });
        validate_config_observation_payload(ConfigObservationKind::ConfiguredIndex, &[], &valid)
            .unwrap();

        for field in ["value", "provenance", "definitions", "configured"] {
            let mut invalid = valid.clone();
            if field == "configured" {
                invalid[field] = serde_json::json!([{
                    "path_components": ["services", "notConfigured"],
                    "key": "b".repeat(64),
                    "configured": false
                }]);
            } else {
                invalid[field] = serde_json::json!("forbidden");
            }
            assert!(
                validate_config_observation_payload(
                    ConfigObservationKind::ConfiguredIndex,
                    &[],
                    &invalid
                )
                .is_err()
            );
        }
    }
}
