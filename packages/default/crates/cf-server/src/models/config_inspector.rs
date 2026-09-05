//! Reconciles the isolated, per-option Config inspector jobs.
//!
//! The checked-in [`config_inspector.nix`](config_inspector.nix) expression
//! selects one real `nixosConfiguration`, discovers its option paths, and
//! creates an index, separate metadata and value jobs, and one independently
//! evaluated raw-provenance carrier. This module does not participate in
//! primary evaluation, persistence, or API serialization.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::deployment_policies::nix_string_pub;
use super::evaluation_snapshots::{SafeEvaluationError, SafeOptionValue};
use crate::security::snapshot_redaction::redact_evaluation_error;

const INDEX_ATTRIBUTE: &str = "__crystalForgeConfigIndex";
const PROVENANCE_ATTRIBUTE: &str = "__crystalForgeProvenance";
const META_PREFIX: &str = "meta_";
const VALUE_PREFIX: &str = "value_";
pub(crate) const EXPECTED_PROVENANCE_ADAPTER_VERSION: u64 = 1;

/// Builds the Nix expression for one exact configuration revision.
///
/// The expression selects `flake.nixosConfigurations[configuration_name]` and
/// passes the selected system's `pkgs.lib` to the dedicated expressions. It
/// never enumerates sibling configurations or evaluates exported modules.
pub(crate) fn build_inspector_expression(flake_ref: &str, configuration_name: &str) -> String {
    let source = include_str!("config_inspector.nix");
    let provenance_source = include_str!("config_provenance.nix");
    format!(
        "let\n  flakeRef = {flake_ref};\n  configurationName = {configuration_name};\n  flake = builtins.getFlake flakeRef;\n  configuration = builtins.getAttr configurationName flake.nixosConfigurations;\n  inspector = ({source}) {{ inherit flakeRef configurationName; }};\n  provenance = ({provenance_source}) {{ inherit flake configuration; }};\nin\n  inspector // {{ {provenance_attribute} = provenance; }}",
        source = source,
        provenance_source = provenance_source,
        provenance_attribute = PROVENANCE_ATTRIBUTE,
        flake_ref = nix_string_pub(flake_ref),
        configuration_name = nix_string_pub(configuration_name),
    )
}

/// Contains the truthful result of one targeted inspection.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ConfigInspectorResult {
    /// Options in the deterministic order supplied by the Nix index.
    pub options: Vec<InspectedOption>,
    /// Raw-definition provenance, or an explicit unavailable state.
    pub provenance: InspectionProvenance,
}

/// Represents one option after independent metadata and value reconciliation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InspectedOption {
    /// Exact option path components supplied by the authoritative Nix index.
    pub path_components: Vec<String>,
    /// The full collision-resistant key emitted by the Nix index.
    pub key: String,
    /// Metadata result, independent from effective value evaluation.
    pub metadata: InspectionMetadata,
    /// Effective value result, independent from metadata evaluation.
    pub value: InspectionValue,
}

/// Describes whether the metadata job produced a result.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InspectionMetadata {
    /// Metadata proved by the selected NixOS option object.
    Available(OptionMetadata),
    /// Metadata job failure retained without evaluator-controlled raw text.
    Failed(SafeEvaluationError),
}

/// Describes whether the effective-value job produced a result.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InspectionValue {
    /// Value encoded by the dedicated value job.
    Available(SafeOptionValue),
    /// Value job failure retained as an explicit safe failure.
    Failed(SafeEvaluationError),
}

/// Describes whether the versioned raw-definition adapter was usable.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InspectionProvenance {
    /// Raw definition metadata was recovered before priority filtering.
    Available {
        /// Adapter protocol version that produced the metadata.
        adapter_version: u64,
        /// Target Nixpkgs library version, when available.
        target_lib_version: Option<String>,
        /// Target module-system source path, when available.
        target_module_system_path: Option<String>,
        /// Validated raw definitions grouped by option key.
        definitions_by_option: Vec<RawDefinitionsForOption>,
    },
    /// Raw-definition provenance was unavailable without invalidating options.
    Unavailable {
        /// Stable reason for the unavailable state.
        reason_code: String,
        /// Sanitized evaluator diagnostic, when one is available.
        diagnostic: Option<SafeEvaluationError>,
    },
}

/// Contains all raw definitions indexed for one exact option path.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RawDefinitionsForOption {
    /// Collision-resistant option key.
    pub option_key: String,
    /// Exact option path components.
    pub path: Vec<String>,
    /// Definitions before priority filtering.
    pub definitions: Vec<RawDefinition>,
}

/// Describes one raw definition without including its value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RawDefinition {
    /// Source module path, when the module system supplied one.
    pub source_path: Option<String>,
    /// Flake input name resolved from the adapter origin table.
    pub source_input: Option<String>,
    /// Full source revision resolved from the adapter origin table.
    pub source_revision: Option<String>,
    /// Normalized module key, when available.
    pub module_key: Option<String>,
    /// Definition ordinal within this option's adapter result.
    pub ordinal: u64,
    /// Target module-system priority.
    pub priority: i64,
    /// Whether the definition survived priority filtering.
    pub status: RawDefinitionStatus,
    /// Position among surviving definitions after target ordering.
    pub surviving_merge_order: Option<u64>,
}

/// Classifies a raw definition using the target module-system result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RawDefinitionStatus {
    /// Definition participates at the winning priority.
    ActiveSurviving,
    /// Definition was discarded by target priority filtering.
    PriorityDiscarded,
}

/// Metadata proved without forcing the option's effective value.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct OptionMetadata {
    /// The option path as components from the authoritative index.
    pub path: Vec<String>,
    /// Internal Nix option marker, when present.
    #[serde(default)]
    pub option_type: Option<String>,
    /// Source location components reported by the module system.
    #[serde(default)]
    pub loc: Vec<String>,
    /// Nix type name, when the option exposes one.
    #[serde(default)]
    pub declared_type: Option<String>,
    /// Declaration source paths.
    #[serde(default)]
    pub declarations: Vec<String>,
    /// Declaration positions, retained as safe JSON metadata.
    #[serde(default)]
    pub declaration_positions: Vec<Value>,
    /// Highest surviving module priority, when defined.
    #[serde(default)]
    pub highest_prio: Option<i64>,
    /// Whether the selected option has a surviving definition.
    #[serde(default)]
    pub is_defined: bool,
    /// Definitions that survived Nix priority filtering, without values.
    #[serde(default)]
    pub surviving_definition_sources: Vec<DefinitionSource>,
}

/// Identifies a surviving option definition without forcing its value.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct DefinitionSource {
    /// Store path reported by the Nix module system.
    pub source_path: String,
    /// Module priority, when exposed by the evaluated definition.
    #[serde(default)]
    pub priority: Option<i64>,
    /// Flake input resolved from the index's safe origin table.
    #[serde(skip)]
    pub source_input: Option<String>,
    /// Full source revision resolved from the index's safe origin table.
    #[serde(skip)]
    pub source_revision: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct JobResult {
    attr: String,
    #[serde(rename = "drvPath")]
    drv_path: Option<String>,
    error: Option<String>,
    #[serde(rename = "extraValue")]
    extra_value: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProvenancePayload {
    #[serde(rename = "adapterVersion")]
    adapter_version: u64,
    supported: bool,
    #[serde(rename = "reasonCode")]
    reason_code: Option<String>,
    #[serde(rename = "targetLibVersion")]
    target_lib_version: Option<String>,
    #[serde(rename = "targetModuleSystemPath")]
    target_module_system_path: Option<String>,
    #[serde(rename = "definitionsByOption", default)]
    definitions_by_option: Vec<RawDefinitionsForOptionPayload>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawDefinitionsForOptionPayload {
    option_key: String,
    path: Vec<String>,
    definitions: Vec<RawDefinitionPayload>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawDefinitionPayload {
    source_path: Option<String>,
    source_input: Option<String>,
    source_revision: Option<String>,
    module_key: Option<String>,
    ordinal: u64,
    priority: i64,
    status: RawDefinitionStatus,
    surviving_merge_order: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct IndexPayload {
    kind: String,
    options: Vec<IndexEntry>,
    #[serde(default)]
    origins: Vec<Origin>,
}

#[derive(Debug, Clone, Deserialize)]
struct IndexEntry {
    key: String,
    path: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Origin {
    name: String,
    #[serde(default)]
    out_path: Option<String>,
    #[serde(default)]
    revision: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MetadataPayload {
    kind: String,
    key: String,
    metadata: OptionMetadata,
}

#[derive(Debug, Clone, Deserialize)]
struct ValuePayload {
    kind: String,
    key: String,
    value: SafeOptionValue,
}

#[derive(Debug, Default)]
struct PendingOption {
    metadata: Option<InspectionMetadata>,
    value: Option<InspectionValue>,
}

/// Reconciles order-independent `nix-eval-jobs` JSONL into targeted options.
///
/// The index is authoritative. A metadata failure cannot remove an option
/// row, and a value failure becomes `not_evaluated` without affecting the
/// metadata result. Successful jobs must share one underlying carrier
/// derivation; this prevents a malformed jobset from mixing systems.
///
/// # Errors
///
/// Returns an error for malformed JSONL, missing or duplicate index entries,
/// unknown inspector attributes, duplicate results, hash mismatches, payload
/// mismatches, or inconsistent successful carrier derivation paths.
pub(crate) fn reconcile_inspector_output(output: &[u8]) -> Result<ConfigInspectorResult> {
    let mut index: Option<IndexPayload> = None;
    let mut pending = HashMap::<String, PendingOption>::new();
    let mut seen_attributes = HashSet::new();
    let mut carrier_drv_path: Option<String> = None;
    let mut provenance: Option<PendingProvenance> = None;

    for (line_number, line) in output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .enumerate()
    {
        let result: JobResult = serde_json::from_slice(line).with_context(|| {
            format!("invalid Config inspector JSONL at line {}", line_number + 1)
        })?;
        if !seen_attributes.insert(result.attr.clone()) {
            bail!("duplicate Config inspector result for {}", result.attr);
        }

        let job = classify_attribute(&result.attr)?;
        if result.error.is_none() {
            let drv_path = result.drv_path.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "successful Config inspector result {} has no drvPath",
                    result.attr
                )
            })?;
            if let Some(expected) = &carrier_drv_path {
                if expected != drv_path {
                    bail!(
                        "Config inspector carrier drvPath changed from {} to {}",
                        expected,
                        drv_path
                    );
                }
            } else {
                carrier_drv_path = Some(drv_path.clone());
            }
        }

        if let Some(error) = result.error {
            match job {
                JobKind::Index => {
                    let failure = failed_evaluation("not_evaluated", &error);
                    bail!("Config inspector index job failed: {}", failure.message)
                }
                JobKind::Metadata(key) => {
                    let failure = failed_evaluation("not_evaluated", &error);
                    pending_entry(&mut pending, key).metadata =
                        Some(InspectionMetadata::Failed(failure));
                }
                JobKind::Value(key) => {
                    let failure = failed_evaluation("not_evaluated", &error);
                    pending_entry(&mut pending, key).value = Some(InspectionValue::Failed(failure));
                }
                JobKind::Provenance => {
                    provenance = Some(PendingProvenance::Unavailable(unavailable_provenance(
                        "adapter_failed",
                        Some(failed_evaluation("adapter_failed", &error)),
                    )));
                }
            }
            continue;
        }

        let Some(payload) = result.extra_value else {
            if matches!(job, JobKind::Provenance) {
                provenance = Some(PendingProvenance::Unavailable(unavailable_provenance(
                    "malformed_payload",
                    None,
                )));
                continue;
            }
            bail!("Config inspector result {} has no extraValue", result.attr);
        };
        match job {
            JobKind::Index => {
                if index.is_some() {
                    bail!("duplicate Config inspector index payload");
                }
                let payload: IndexPayload = serde_json::from_value(payload)
                    .context("invalid Config inspector index payload")?;
                validate_index(&payload)?;
                index = Some(payload);
            }
            JobKind::Metadata(key) => {
                let payload: MetadataPayload = serde_json::from_value(payload)
                    .with_context(|| format!("invalid metadata payload for {key}"))?;
                if payload.kind != "metadata" || payload.key != key {
                    bail!("metadata payload does not match job key {key}");
                }
                if option_key(&payload.metadata.path) != key {
                    bail!("metadata path hash does not match job key {key}");
                }
                let entry = pending_entry(&mut pending, key);
                if entry.metadata.is_some() {
                    bail!("duplicate metadata result for {key}");
                }
                entry.metadata = Some(InspectionMetadata::Available(payload.metadata));
            }
            JobKind::Value(key) => {
                let payload: ValuePayload = serde_json::from_value(payload)
                    .with_context(|| format!("invalid value payload for {key}"))?;
                if payload.kind != "value" || payload.key != key {
                    bail!("value payload does not match job key {key}");
                }
                let entry = pending_entry(&mut pending, key);
                if entry.value.is_some() {
                    bail!("duplicate value result for {key}");
                }
                entry.value = Some(InspectionValue::Available(payload.value));
            }
            JobKind::Provenance => {
                let parsed = serde_json::from_value::<ProvenancePayload>(payload);
                match parsed {
                    Ok(payload) => {
                        if provenance.is_some() {
                            bail!("duplicate Config inspector provenance result");
                        }
                        provenance = Some(PendingProvenance::Payload(payload));
                    }
                    Err(_) => {
                        provenance = Some(PendingProvenance::Unavailable(unavailable_provenance(
                            "malformed_payload",
                            None,
                        )));
                    }
                }
            }
        }
    }

    let index = index.ok_or_else(|| anyhow::anyhow!("Config inspector index result is missing"))?;
    let origins = index.origins.clone();
    let provenance = match provenance
        .ok_or_else(|| anyhow::anyhow!("Config inspector provenance result is missing"))?
    {
        PendingProvenance::Unavailable(result) => result,
        PendingProvenance::Payload(payload) => reconcile_provenance(payload, &index),
    };
    let mut options = Vec::with_capacity(index.options.len());
    for entry in index.options {
        let pending = pending.remove(&entry.key).ok_or_else(|| {
            anyhow::anyhow!(
                "Config inspector results are missing jobs for {}",
                entry.key
            )
        })?;
        let metadata = pending.metadata.ok_or_else(|| {
            anyhow::anyhow!(
                "Config inspector metadata result is missing for {}",
                entry.key
            )
        })?;
        let key = entry.key;
        let metadata = resolve_definition_origins(metadata, &origins);
        options.push(InspectedOption {
            path_components: entry.path,
            key: key.clone(),
            metadata,
            value: pending.value.ok_or_else(|| {
                anyhow::anyhow!("Config inspector value result is missing for {}", key)
            })?,
        });
    }

    if let Some(unindexed) = pending.keys().next() {
        bail!("Config inspector result hash {unindexed} is not in the index");
    }

    Ok(ConfigInspectorResult {
        options,
        provenance,
    })
}

fn resolve_definition_origins(
    metadata: InspectionMetadata,
    origins: &[Origin],
) -> InspectionMetadata {
    let InspectionMetadata::Available(mut metadata) = metadata else {
        return metadata;
    };
    for definition in &mut metadata.surviving_definition_sources {
        let origin = origins
            .iter()
            .filter_map(|origin| {
                let out_path = origin.out_path.as_deref()?;
                let matches = definition.source_path == out_path
                    || definition.source_path.starts_with(&format!("{out_path}/"));
                matches.then_some((out_path.len(), origin))
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, origin)| origin);
        if let Some(origin) = origin {
            definition.source_input = Some(origin.name.clone());
            definition.source_revision = origin.revision.clone();
        }
    }
    InspectionMetadata::Available(metadata)
}

fn validate_index(index: &IndexPayload) -> Result<()> {
    if index.kind != "index" {
        bail!("Config inspector index payload has invalid kind");
    }
    let mut keys = HashSet::new();
    let mut paths = HashSet::new();
    for entry in &index.options {
        if !keys.insert(entry.key.clone()) {
            bail!("duplicate Config inspector index key {}", entry.key);
        }
        if !paths.insert(entry.path.clone()) {
            bail!(
                "duplicate Config inspector index path {}",
                entry.path.join(".")
            );
        }
        if option_key(&entry.path) != entry.key {
            bail!("Config inspector index key does not hash its path");
        }
    }
    Ok(())
}

fn pending_entry<'a>(
    pending: &'a mut HashMap<String, PendingOption>,
    key: &str,
) -> &'a mut PendingOption {
    pending.entry(key.to_string()).or_default()
}

enum JobKind<'a> {
    Index,
    Metadata(&'a str),
    Value(&'a str),
    Provenance,
}

fn classify_attribute(attribute: &str) -> Result<JobKind<'_>> {
    if attribute == INDEX_ATTRIBUTE {
        return Ok(JobKind::Index);
    }
    if attribute == PROVENANCE_ATTRIBUTE {
        return Ok(JobKind::Provenance);
    }
    if let Some(key) = attribute.strip_prefix(META_PREFIX) {
        validate_key(key)?;
        return Ok(JobKind::Metadata(key));
    }
    if let Some(key) = attribute.strip_prefix(VALUE_PREFIX) {
        validate_key(key)?;
        return Ok(JobKind::Value(key));
    }
    bail!("unknown Config inspector job attribute {attribute}");
}

enum PendingProvenance {
    Payload(ProvenancePayload),
    Unavailable(InspectionProvenance),
}

fn unavailable_provenance(
    reason_code: &str,
    diagnostic: Option<SafeEvaluationError>,
) -> InspectionProvenance {
    InspectionProvenance::Unavailable {
        reason_code: reason_code.to_string(),
        diagnostic,
    }
}

fn reconcile_provenance(payload: ProvenancePayload, index: &IndexPayload) -> InspectionProvenance {
    if payload.adapter_version != EXPECTED_PROVENANCE_ADAPTER_VERSION {
        return unavailable_provenance("unsupported_adapter_version", None);
    }
    if !payload.supported {
        let reason_code = payload
            .reason_code
            .as_deref()
            .filter(|reason| {
                matches!(
                    *reason,
                    "module_type_payload_unavailable"
                        | "capability_self_test_failed"
                        | "graph_replay_mismatch"
                        | "adapter_evaluation_failed"
                        | "provenance_integrity_failure"
                )
            })
            .unwrap_or("adapter_unsupported");
        return unavailable_provenance(reason_code, None);
    }

    let index_paths = index
        .options
        .iter()
        .map(|entry| (entry.key.as_str(), entry.path.as_slice()))
        .collect::<HashMap<_, _>>();
    let mut seen_options = HashSet::new();
    let mut definitions_by_option = Vec::with_capacity(payload.definitions_by_option.len());

    for option in payload.definitions_by_option {
        if !seen_options.insert(option.option_key.clone())
            || option_key(&option.path) != option.option_key
            || index_paths
                .get(option.option_key.as_str())
                .is_none_or(|path| *path != option.path.as_slice())
        {
            return unavailable_provenance("provenance_integrity_failure", None);
        }

        let mut ordinals = HashSet::new();
        let mut merge_orders = HashSet::new();
        let mut definitions = Vec::with_capacity(option.definitions.len());
        for definition in option.definitions {
            if !ordinals.insert(definition.ordinal) {
                return unavailable_provenance("provenance_integrity_failure", None);
            }
            match definition.status {
                RawDefinitionStatus::ActiveSurviving => {
                    let Some(order) = definition.surviving_merge_order else {
                        return unavailable_provenance("provenance_integrity_failure", None);
                    };
                    if !merge_orders.insert(order) {
                        return unavailable_provenance("provenance_integrity_failure", None);
                    }
                }
                RawDefinitionStatus::PriorityDiscarded => {
                    if definition.surviving_merge_order.is_some() {
                        return unavailable_provenance("provenance_integrity_failure", None);
                    }
                }
            }
            definitions.push(RawDefinition {
                source_path: definition.source_path,
                source_input: definition.source_input,
                source_revision: definition.source_revision,
                module_key: definition.module_key,
                ordinal: definition.ordinal,
                priority: definition.priority,
                status: definition.status,
                surviving_merge_order: definition.surviving_merge_order,
            });
        }
        definitions_by_option.push(RawDefinitionsForOption {
            option_key: option.option_key,
            path: option.path,
            definitions,
        });
    }

    InspectionProvenance::Available {
        adapter_version: payload.adapter_version,
        target_lib_version: payload.target_lib_version,
        target_module_system_path: payload.target_module_system_path,
        definitions_by_option,
    }
}

fn validate_key(key: &str) -> Result<()> {
    if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid Config inspector option hash {key}");
    }
    Ok(())
}

fn option_key(path: &[String]) -> String {
    let bytes = serde_json::to_vec(path).expect("JSON arrays of strings are serializable");
    hex::encode(Sha256::digest(bytes))
}

fn failed_evaluation(code: &str, message: &str) -> SafeEvaluationError {
    SafeEvaluationError {
        code: code.to_string(),
        message: redact_evaluation_error(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hash(path: &[&str]) -> String {
        option_key(
            &path
                .iter()
                .map(|part| (*part).to_string())
                .collect::<Vec<_>>(),
        )
    }

    fn result(attr: &str, payload: Value) -> Value {
        json!({
            "attr": attr,
            "attrPath": [attr],
            "drvPath": "/nix/store/shared.drv",
            "error": null,
            "extraValue": payload,
        })
    }

    fn reconcile_for_test(output: &str) -> Result<ConfigInspectorResult> {
        if output.contains(PROVENANCE_ATTRIBUTE) {
            reconcile_inspector_output(output.as_bytes())
        } else {
            let provenance = result(
                PROVENANCE_ATTRIBUTE,
                json!({
                    "adapterVersion": EXPECTED_PROVENANCE_ADAPTER_VERSION,
                    "supported": false,
                    "reasonCode": "capability_self_test_failed",
                }),
            );
            reconcile_inspector_output(format!("{output}\n{provenance}").as_bytes())
        }
    }

    fn index(key: &str, path: &[&str]) -> Value {
        json!({
            "kind": "index",
            "options": [{
                "key": key,
                "path": path,
            }],
            "origins": [],
        })
    }

    fn metadata(key: &str, path: &[&str]) -> Value {
        json!({
            "kind": "metadata",
            "key": key,
            "metadata": {
                "path": path,
                "option_type": "option",
                "loc": path,
                "declared_type": "bool",
                "declarations": ["/nix/store/source/module.nix"],
                "declaration_positions": [],
                "highest_prio": 100,
                "is_defined": true,
                "surviving_definition_sources": [{
                    "source_path": "/nix/store/source/module.nix",
                    "priority": 100,
                }],
            },
        })
    }

    #[test]
    fn expression_selects_one_configuration_and_uses_selected_system_library() {
        let expression = build_inspector_expression("path:/tmp/example", "good");

        assert!(
            expression.contains("builtins.getAttr configurationName flake.nixosConfigurations")
        );
        assert!(expression.contains("configuration.pkgs.lib"));
        assert!(expression.contains("__crystalForgeProvenance"));
        assert!(expression.contains("provenanceAdapterVersion = 1"));
        assert!(!expression.contains("flake.inputs.nixpkgs"));
        assert!(!expression.contains("flake.nixosModules"));
        assert!(!expression.contains("evaluationSnapshot"));
    }

    #[test]
    fn expression_uses_safe_nix_strings_for_flake_and_configuration_names() {
        let value = r#"${builtins.abort "should-not-run"} \ "quoted"
	"#;
        let expression = build_inspector_expression(value, value);

        assert!(!expression.contains("flakeRef = \"${builtins.abort"));
        assert!(!expression.contains("configurationName = \"${builtins.abort"));
        assert!(expression.contains("flakeRef = \"\\${builtins.abort"));
        assert!(expression.contains("configurationName = \"\\${builtins.abort"));
        assert!(expression.contains("\\\"quoted\\\""));
        assert!(expression.contains("\\\\"));
        assert!(expression.contains("\\n"));
        assert!(expression.contains("\\t"));
    }

    #[test]
    fn expression_preserves_canonical_nul_behavior() {
        let expression = build_inspector_expression("before\0after", "good");

        assert!(expression.contains("flakeRef = throw \"Nix strings cannot represent NUL bytes\""));
    }

    #[test]
    fn reconciliation_is_order_independent_and_preserves_value_failure() {
        let path = ["crystalForgeProbe", "poison"];
        let key = hash(&path);
        let lines = [
            result(
                &format!("value_{key}"),
                json!({
                    "kind": "value",
                    "key": key,
                    "value": { "kind": "scalar", "value": true },
                }),
            ),
            result("__crystalForgeConfigIndex", index(&key, &path)),
            json!({
                "attr": format!("meta_{key}"),
                "attrPath": [format!("meta_{key}")],
                "drvPath": "/nix/store/shared.drv",
                "error": "selected option contains token=secret",
            }),
        ];
        let output = lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        let result = reconcile_for_test(&output).unwrap();
        assert_eq!(result.options.len(), 1);
        assert!(matches!(
            result.options[0].metadata,
            InspectionMetadata::Failed(_)
        ));
        assert!(matches!(
            result.options[0].value,
            InspectionValue::Available(SafeOptionValue::Scalar(_))
        ));
        assert!(!format!("{:?}", result.options[0]).contains("secret"));
    }

    #[test]
    fn metadata_failure_does_not_hide_successful_value() {
        let path = ["healthy", "option"];
        let key = hash(&path);
        let lines = [
            result("__crystalForgeConfigIndex", index(&key, &path)),
            result(
                &format!("value_{key}"),
                json!({
                    "kind": "value",
                    "key": key,
                    "value": { "kind": "scalar", "value": "ok" },
                }),
            ),
            json!({
                "attr": format!("meta_{key}"),
                "attrPath": [format!("meta_{key}")],
                "drvPath": "/nix/store/shared.drv",
                "error": "metadata failed",
            }),
        ];
        let output = lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        let result = reconcile_for_test(&output).unwrap();
        assert!(matches!(
            result.options[0].metadata,
            InspectionMetadata::Failed(_)
        ));
        assert!(matches!(
            result.options[0].value,
            InspectionValue::Available(SafeOptionValue::Scalar(_))
        ));
    }

    #[test]
    fn successful_metadata_survives_an_independent_value_failure() {
        let path = ["poisoned", "value"];
        let key = hash(&path);
        let lines = [
            result("__crystalForgeConfigIndex", index(&key, &path)),
            result(&format!("meta_{key}"), metadata(&key, &path)),
            json!({
                "attr": format!("value_{key}"),
                "attrPath": [format!("value_{key}")],
                "drvPath": null,
                "error": "selected option value failed with token=secret",
            }),
        ];
        let output = lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        let result = reconcile_for_test(&output).unwrap();
        assert!(matches!(
            result.options[0].metadata,
            InspectionMetadata::Available(_)
        ));
        let InspectionValue::Failed(error) = &result.options[0].value else {
            panic!("value failure was not preserved");
        };
        assert_eq!(error.code, "not_evaluated");
        assert!(!error.message.contains("secret"));
    }

    #[test]
    fn missing_metadata_or_value_results_are_integrity_failures() {
        let path = ["healthy", "option"];
        let key = hash(&path);
        let index_line = result("__crystalForgeConfigIndex", index(&key, &path));
        let metadata_line = result(&format!("meta_{key}"), metadata(&key, &path));
        let value_line = result(
            &format!("value_{key}"),
            json!({
                "kind": "value",
                "key": key,
                "value": { "kind": "scalar", "value": true },
            }),
        );

        for lines in [
            vec![index_line.clone(), metadata_line.clone()],
            vec![index_line.clone(), value_line.clone()],
            vec![index_line.clone()],
        ] {
            let output = lines
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(reconcile_for_test(&output).is_err());
        }
    }

    #[test]
    fn truncated_multi_option_output_is_an_integrity_failure() {
        let first = ["first", "option"];
        let second = ["second", "option"];
        let first_key = hash(&first);
        let second_key = hash(&second);
        let lines = [
            result(
                "__crystalForgeConfigIndex",
                json!({
                    "kind": "index",
                    "options": [
                        { "key": first_key, "path": first },
                        { "key": second_key, "path": second },
                    ],
                    "origins": [],
                }),
            ),
            result(&format!("meta_{first_key}"), metadata(&first_key, &first)),
            result(
                &format!("value_{first_key}"),
                json!({
                    "kind": "value",
                    "key": first_key,
                    "value": { "kind": "scalar", "value": true },
                }),
            ),
        ];
        let output = lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(reconcile_for_test(&output).is_err());
    }

    #[test]
    fn exact_path_components_survive_reconciliation_and_hash_distinctly() {
        let dotted = ["foo", "bar", "baz"];
        let dotted_component = ["foo", "bar.baz"];
        let dotted_key = hash(&dotted);
        let dotted_component_key = hash(&dotted_component);
        assert_ne!(dotted_key, dotted_component_key);

        let special = ["quote\" whitespace", r#"${not-an-interpolation}"#];
        let special_key = hash(&special);
        let lines = [
            result(
                "__crystalForgeConfigIndex",
                json!({
                    "kind": "index",
                    "options": [
                        { "key": dotted_key, "path": dotted },
                        { "key": dotted_component_key, "path": dotted_component },
                        { "key": special_key, "path": special },
                    ],
                    "origins": [],
                }),
            ),
            result(
                &format!("meta_{dotted_key}"),
                metadata(&dotted_key, &dotted),
            ),
            result(
                &format!("value_{dotted_key}"),
                json!({
                    "kind": "value", "key": dotted_key,
                    "value": { "kind": "scalar", "value": true },
                }),
            ),
            result(
                &format!("meta_{dotted_component_key}"),
                metadata(&dotted_component_key, &dotted_component),
            ),
            result(
                &format!("value_{dotted_component_key}"),
                json!({
                    "kind": "value", "key": dotted_component_key,
                    "value": { "kind": "scalar", "value": true },
                }),
            ),
            result(
                &format!("meta_{special_key}"),
                metadata(&special_key, &special),
            ),
            result(
                &format!("value_{special_key}"),
                json!({
                    "kind": "value", "key": special_key,
                    "value": { "kind": "scalar", "value": true },
                }),
            ),
        ];
        let output = lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let result = reconcile_for_test(&output).unwrap();

        assert_eq!(result.options[0].path_components, string_path(&dotted));
        assert_eq!(
            result.options[1].path_components,
            string_path(&dotted_component)
        );
        assert_eq!(result.options[2].path_components, string_path(&special));
    }

    fn string_path(path: &[&str]) -> Vec<String> {
        path.iter().map(|part| (*part).to_string()).collect()
    }

    #[test]
    fn duplicate_and_unknown_jobs_are_rejected() {
        let path = ["example", "option"];
        let key = hash(&path);
        let index_line = result("__crystalForgeConfigIndex", index(&key, &path)).to_string();
        let duplicate = result(
            &format!("value_{key}"),
            json!({
                "kind": "value",
                "key": key,
                "value": { "kind": "scalar", "value": true },
            }),
        )
        .to_string();
        let duplicate_output = format!("{index_line}\n{duplicate}\n{duplicate}");
        assert!(reconcile_for_test(&duplicate_output).is_err());

        let unknown = result("other_job", json!({})).to_string();
        assert!(reconcile_for_test(&unknown).is_err());
    }

    #[test]
    fn successful_carriers_must_share_one_drv_path() {
        let path = ["example", "option"];
        let key = hash(&path);
        let mut index_line = result("__crystalForgeConfigIndex", index(&key, &path));
        index_line["drvPath"] = json!("/nix/store/one.drv");
        let mut metadata_line = result(&format!("meta_{key}"), metadata(&key, &path));
        metadata_line["drvPath"] = json!("/nix/store/two.drv");
        let output = format!("{}\n{}", index_line, metadata_line);

        assert!(reconcile_for_test(&output).is_err());
    }

    #[test]
    fn successful_result_without_drv_path_is_rejected() {
        let path = ["example", "option"];
        let key = hash(&path);
        let mut line = result("__crystalForgeConfigIndex", index(&key, &path));
        line["drvPath"] = Value::Null;

        assert!(reconcile_for_test(&line.to_string()).is_err());
    }

    fn supported_provenance(path: &[&str], key: &str) -> Value {
        json!({
            "adapterVersion": EXPECTED_PROVENANCE_ADAPTER_VERSION,
            "supported": true,
            "targetLibVersion": "26.05pre-git",
            "targetModuleSystemPath": "/nix/store/nixpkgs",
            "definitionsByOption": [{
                "option_key": key,
                "path": path,
                "definitions": [
                    {
                        "source_path": "/nix/store/source/default.nix",
                        "source_input": "self",
                        "source_revision": null,
                        "module_key": "module",
                        "ordinal": 0,
                        "priority": 1000,
                        "status": "priority_discarded",
                        "surviving_merge_order": null,
                    },
                    {
                        "source_path": "/nix/store/source/force.nix",
                        "source_input": "self",
                        "source_revision": null,
                        "module_key": "force",
                        "ordinal": 1,
                        "priority": 50,
                        "status": "active_surviving",
                        "surviving_merge_order": 0,
                    },
                ],
            }],
        })
    }

    #[test]
    fn supported_provenance_preserves_discarded_and_surviving_definitions() {
        let path = ["crystalForgeProbe", "target"];
        let key = hash(&path);
        let lines = [
            result("__crystalForgeConfigIndex", index(&key, &path)),
            result(&format!("meta_{key}"), metadata(&key, &path)),
            result(
                &format!("value_{key}"),
                json!({
                    "kind": "value",
                    "key": key,
                    "value": { "kind": "scalar", "value": "winner" },
                }),
            ),
            result(PROVENANCE_ATTRIBUTE, supported_provenance(&path, &key)),
        ];
        let output = lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let result = reconcile_inspector_output(output.as_bytes()).unwrap();

        let InspectionProvenance::Available {
            adapter_version,
            definitions_by_option,
            ..
        } = result.provenance
        else {
            panic!("supported provenance was not available");
        };
        assert_eq!(adapter_version, EXPECTED_PROVENANCE_ADAPTER_VERSION);
        assert_eq!(definitions_by_option.len(), 1);
        assert_eq!(definitions_by_option[0].definitions.len(), 2);
        assert_eq!(
            definitions_by_option[0].definitions[0].status,
            RawDefinitionStatus::PriorityDiscarded
        );
        assert_eq!(
            definitions_by_option[0].definitions[1].status,
            RawDefinitionStatus::ActiveSurviving
        );
    }

    #[test]
    fn explicit_provenance_error_does_not_hide_core_inspection() {
        let path = ["healthy", "option"];
        let key = hash(&path);
        let lines = [
            result("__crystalForgeConfigIndex", index(&key, &path)),
            result(&format!("meta_{key}"), metadata(&key, &path)),
            result(
                &format!("value_{key}"),
                json!({
                    "kind": "value",
                    "key": key,
                    "value": { "kind": "scalar", "value": true },
                }),
            ),
            json!({
                "attr": PROVENANCE_ATTRIBUTE,
                "attrPath": [PROVENANCE_ATTRIBUTE],
                "drvPath": null,
                "error": "provenance adapter failed with token=secret",
            }),
        ];
        let output = lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let result = reconcile_inspector_output(output.as_bytes()).unwrap();

        assert!(matches!(
            result.options[0].metadata,
            InspectionMetadata::Available(_)
        ));
        assert!(matches!(
            result.options[0].value,
            InspectionValue::Available(_)
        ));
        assert!(matches!(
            result.provenance,
            InspectionProvenance::Unavailable { ref reason_code, .. }
                if reason_code == "adapter_failed"
        ));
    }

    #[test]
    fn provenance_integrity_failure_isolated_from_core_inspection() {
        let path = ["healthy", "option"];
        let key = hash(&path);
        let mut payload = supported_provenance(&path, &key);
        payload["definitionsByOption"][0]["definitions"][1]["ordinal"] = json!(0);
        let lines = [
            result("__crystalForgeConfigIndex", index(&key, &path)),
            result(&format!("meta_{key}"), metadata(&key, &path)),
            result(
                &format!("value_{key}"),
                json!({
                    "kind": "value",
                    "key": key,
                    "value": { "kind": "scalar", "value": true },
                }),
            ),
            result(PROVENANCE_ATTRIBUTE, payload),
        ];
        let output = lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let result = reconcile_inspector_output(output.as_bytes()).unwrap();

        assert!(matches!(
            result.provenance,
            InspectionProvenance::Unavailable { ref reason_code, .. }
                if reason_code == "provenance_integrity_failure"
        ));
        assert_eq!(result.options.len(), 1);
    }

    #[test]
    fn missing_provenance_result_is_a_stream_integrity_failure() {
        let path = ["healthy", "option"];
        let key = hash(&path);
        let lines = [
            result("__crystalForgeConfigIndex", index(&key, &path)),
            result(&format!("meta_{key}"), metadata(&key, &path)),
            result(
                &format!("value_{key}"),
                json!({
                    "kind": "value",
                    "key": key,
                    "value": { "kind": "scalar", "value": true },
                }),
            ),
        ];
        let output = lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(reconcile_inspector_output(output.as_bytes()).is_err());
    }

    #[test]
    fn unsupported_provenance_preserves_core_inspection() {
        let path = ["healthy", "option"];
        let key = hash(&path);
        let lines = [
            result("__crystalForgeConfigIndex", index(&key, &path)),
            result(&format!("meta_{key}"), metadata(&key, &path)),
            result(
                &format!("value_{key}"),
                json!({
                    "kind": "value",
                    "key": key,
                    "value": { "kind": "scalar", "value": true },
                }),
            ),
        ];
        let output = lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let result = reconcile_for_test(&output).unwrap();

        assert_eq!(result.options.len(), 1);
        assert!(matches!(
            result.provenance,
            InspectionProvenance::Unavailable { ref reason_code, .. }
                if reason_code == "capability_self_test_failed"
        ));
    }
}
