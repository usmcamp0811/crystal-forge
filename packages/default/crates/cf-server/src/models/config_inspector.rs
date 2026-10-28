//! Reconciles the isolated, per-option Config inspector jobs.
//!
//! The checked-in [`config_inspector.nix`](config_inspector.nix) expression
//! selects one real `nixosConfiguration`, discovers its option paths, and
//! creates an index, separate metadata and value jobs, and one independently
//! evaluated raw-provenance carrier. This module does not participate in
//! primary evaluation, persistence, or API serialization.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::deployment_policies::nix_string_pub;
use super::evaluation_snapshots::{SafeEvaluationError, SafeOptionValue};
use crate::security::snapshot_redaction::redact_evaluation_error;

const INDEX_ATTRIBUTE: &str = "__crystalForgeConfigIndex";
const PROVENANCE_ATTRIBUTE: &str = "__crystalForgeProvenance";
const DEFINITION_INDEX_ATTRIBUTE: &str = "__crystalForgeDefinitionIndex";
const DEFINITION_VALUE_PREFIX: &str = "def_value_";
const META_PREFIX: &str = "meta_";
const VALUE_PREFIX: &str = "value_";
pub(crate) const EXPECTED_PROVENANCE_ADAPTER_VERSION: u64 = 1;
const MAX_OPTION_INVENTORY_DIAGNOSTICS: usize = 128;
const MAX_OPTION_INVENTORY_PATH_COMPONENTS: usize = 16;
const MAX_OPTION_INVENTORY_PATH_COMPONENT_CHARS: usize = 256;

/// Identifies the exact requested configuration shared by both inspector stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectionTarget {
    /// Flake reference passed to `builtins.getFlake`.
    pub flake_ref: String,
    /// Configuration name selected from the flake.
    pub configuration_name: String,
    /// Canonical key for the requested flake/configuration pair.
    pub target_key: String,
}

impl InspectionTarget {
    /// Creates an immutable inspection target and computes its canonical key.
    pub(crate) fn new(flake_ref: &str, configuration_name: &str) -> Self {
        Self {
            flake_ref: flake_ref.to_string(),
            configuration_name: configuration_name.to_string(),
            target_key: inspection_target_key(flake_ref, configuration_name),
        }
    }
}

/// Computes the internal key that binds both inspection stages to one target.
pub(crate) fn inspection_target_key(flake_ref: &str, configuration_name: &str) -> String {
    let encoded = serde_json::to_vec(&[flake_ref, configuration_name])
        .expect("JSON encoding of string inspection target components cannot fail");
    format_digest(&encoded)
}

fn format_digest(input: &[u8]) -> String {
    let digest = Sha256::digest(input);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Builds the Nix expression for one exact configuration revision.
///
/// The expression resolves one flake/configuration pair and passes those exact
/// bindings to the inspector and provenance expressions. It never enumerates
/// sibling configurations or evaluates exported modules.
pub(crate) fn build_inspector_expression(target: &InspectionTarget) -> String {
    let source = include_str!("config_inspector.nix");
    let provenance_lib_source = include_str!("config_provenance_lib.nix");
    let value_encoding_source = include_str!("config_value_encoding.nix");
    format!(
        "let\n  flakeRef = {flake_ref};\n  configurationName = {configuration_name};\n  targetKey = {target_key};\n  flake = builtins.getFlake flakeRef;\n  configuration = builtins.getAttr configurationName flake.nixosConfigurations;\n  valueEncoder = ({value_encoding_source});\nin\n  ({source}) {{ inherit flake configuration targetKey; provenanceLib = ({provenance_lib_source}); encodeValue = valueEncoder configuration.pkgs.lib; }}",
        source = source,
        provenance_lib_source = provenance_lib_source,
        value_encoding_source = value_encoding_source,
        target_key = nix_string_pub(&target.target_key),
        flake_ref = nix_string_pub(&target.flake_ref),
        configuration_name = nix_string_pub(&target.configuration_name),
    )
}

/// Builds the separate raw-definition value job expression for one exact
/// configuration revision.
///
/// The expression reuses the provenance replay and safe-value encoder but
/// remains a separate nix-eval-jobs root. Every value job uses the selected
/// configuration's `system.build.toplevel` as its carrier. It reads the exact
/// allowed option keys and paths from the specified JSON file under impure
/// evaluation. The caller must keep that owner-only file alive until the
/// evaluator exits.
pub(crate) fn build_definition_values_expression(
    target: &InspectionTarget,
    allowed_selection_path: &str,
) -> String {
    let source = include_str!("config_definition_values.nix");
    let provenance_lib_source = include_str!("config_provenance_lib.nix");
    let value_encoding_source = include_str!("config_value_encoding.nix");
    format!(
        "let\n  flakeRef = {flake_ref};\n  configurationName = {configuration_name};\n  targetKey = {target_key};\n  allowedSelection = builtins.fromJSON (builtins.readFile {allowed_selection_path});\n  allowedOptionKeys = allowedSelection.optionKeys;\n  allowedOptionPaths = allowedSelection.optionPaths;\n  flake = builtins.getFlake flakeRef;\n  configuration = builtins.getAttr configurationName flake.nixosConfigurations;\n  valueEncoder = ({value_encoding_source});\n  jobs = ({source}) {{ inherit flake configuration targetKey allowedOptionKeys allowedOptionPaths; provenanceLib = ({provenance_lib_source}); encodeValue = valueEncoder configuration.pkgs.lib; }};\nin\njobs",
        source = source,
        provenance_lib_source = provenance_lib_source,
        value_encoding_source = value_encoding_source,
        target_key = nix_string_pub(&target.target_key),
        flake_ref = nix_string_pub(&target.flake_ref),
        configuration_name = nix_string_pub(&target.configuration_name),
        allowed_selection_path = nix_string_pub(allowed_selection_path),
    )
}

/// Contains the truthful result of one targeted inspection.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ConfigInspectorResult {
    /// Internal key binding both stages to the requested flake/configuration pair.
    pub target_key: String,
    /// Validated carrier derivation for the complete Stage-1 inspection.
    pub carrier_drv_path: String,
    /// Resolved immutable source path returned by `builtins.getFlake`.
    pub source_out_path: String,
    /// Whether Stage 1 enumerated the complete option tree.
    pub option_inventory_complete: bool,
    /// Bounded unreadable option-tree prefixes in deterministic traversal order.
    pub option_inventory_diagnostics: Vec<OptionInventoryDiagnostic>,
    /// Whether additional diagnostics existed after the retained detail budget.
    pub option_inventory_diagnostics_truncated: bool,
    /// Options in the deterministic order supplied by the Nix index.
    pub options: Vec<InspectedOption>,
    /// Raw-definition provenance, or an explicit unavailable state.
    pub provenance: InspectionProvenance,
}

/// Identifies one option-tree prefix that Stage 1 could not enumerate.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, serde::Serialize)]
pub(crate) struct OptionInventoryDiagnostic {
    /// Exact unreadable option-tree prefix components.
    pub path: Vec<String>,
    /// Stable failure category.
    pub code: String,
    /// Stable diagnostic. Artifact conversion redacts its dynamic path first.
    pub message: String,
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
        /// Digest of the canonical raw-definition provenance metadata.
        provenance_digest: String,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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
    #[serde(rename = "provenanceDigest", default)]
    provenance_digest: Option<String>,
    #[serde(rename = "definitionsByOption")]
    definitions_by_option: Option<Vec<RawDefinitionsForOptionPayload>>,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CanonicalProvenance<'a> {
    definitions_by_option: Vec<CanonicalDefinitionsForOption<'a>>,
    provenance_adapter_version: u64,
    target_lib_version: &'a Option<String>,
    target_module_system_path: &'a Option<String>,
}

#[derive(Serialize)]
struct CanonicalDefinitionsForOption<'a> {
    definitions: Vec<CanonicalRawDefinition<'a>>,
    option_key: &'a str,
    path: &'a [String],
}

#[derive(Serialize)]
struct CanonicalRawDefinition<'a> {
    module_key: &'a Option<String>,
    ordinal: u64,
    priority: i64,
    source_input: &'a Option<String>,
    source_path: &'a Option<String>,
    source_revision: &'a Option<String>,
    status: RawDefinitionStatus,
    surviving_merge_order: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct IndexPayload {
    kind: String,
    #[serde(rename = "targetKey")]
    target_key: String,
    #[serde(rename = "sourceOutPath")]
    source_out_path: String,
    options: Vec<IndexEntry>,
    #[serde(rename = "optionInventoryComplete")]
    option_inventory_complete: bool,
    #[serde(rename = "optionInventoryDiagnostics")]
    option_inventory_diagnostics: Vec<OptionInventoryDiagnostic>,
    #[serde(rename = "optionInventoryDiagnosticsTruncated")]
    option_inventory_diagnostics_truncated: bool,
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
pub(crate) fn reconcile_inspector_output(
    output: &[u8],
    expected_target: &InspectionTarget,
) -> Result<ConfigInspectorResult> {
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
    validate_key(&index.target_key)?;
    if index.target_key != expected_target.target_key {
        bail!("Config inspector target key does not match expected target");
    }
    validate_source_out_path(&index.source_out_path)?;
    let target_key = index.target_key.clone();
    let source_out_path = index.source_out_path.clone();
    let carrier_drv_path = carrier_drv_path
        .ok_or_else(|| anyhow::anyhow!("Config inspector carrier drvPath is missing"))?;
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
        target_key,
        carrier_drv_path,
        source_out_path,
        option_inventory_complete: index.option_inventory_complete,
        option_inventory_diagnostics: index.option_inventory_diagnostics,
        option_inventory_diagnostics_truncated: index.option_inventory_diagnostics_truncated,
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
    validate_raw_option_inventory(
        index.option_inventory_complete,
        &index.option_inventory_diagnostics,
        index.option_inventory_diagnostics_truncated,
    )?;
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

pub(crate) fn validate_option_inventory(
    complete: bool,
    diagnostics: &[OptionInventoryDiagnostic],
    diagnostics_truncated: bool,
) -> Result<()> {
    validate_option_inventory_impl(complete, diagnostics, diagnostics_truncated, true)
}

fn validate_raw_option_inventory(
    complete: bool,
    diagnostics: &[OptionInventoryDiagnostic],
    diagnostics_truncated: bool,
) -> Result<()> {
    validate_option_inventory_impl(complete, diagnostics, diagnostics_truncated, false)
}

fn validate_option_inventory_impl(
    complete: bool,
    diagnostics: &[OptionInventoryDiagnostic],
    diagnostics_truncated: bool,
    require_persistable_components: bool,
) -> Result<()> {
    if complete != (diagnostics.is_empty() && !diagnostics_truncated) {
        bail!("Config inspector option inventory state contradicts its diagnostics");
    }
    if !complete && diagnostics.is_empty() {
        bail!("partial Config inspector option inventory has no diagnostic detail");
    }
    if diagnostics.len() > MAX_OPTION_INVENTORY_DIAGNOSTICS {
        bail!("Config inspector option inventory diagnostics exceed the limit");
    }
    let mut previous: Option<Vec<u8>> = None;
    for diagnostic in diagnostics {
        if diagnostic.path.is_empty()
            || diagnostic.path.len() > MAX_OPTION_INVENTORY_PATH_COMPONENTS
            || (require_persistable_components
                && diagnostic.path.iter().any(|component| {
                    component.is_empty()
                        || component.chars().count() > MAX_OPTION_INVENTORY_PATH_COMPONENT_CHARS
                }))
        {
            bail!("Config inspector option inventory diagnostic path is invalid");
        }
        let expected_message = match diagnostic.code.as_str() {
            "unreadable_option_subtree" => "Option subtree could not be inspected",
            "option_subtree_depth_exceeded" => "Option subtree exceeds the traversal depth limit",
            _ => bail!("Config inspector option inventory diagnostic code is invalid"),
        };
        if diagnostic.message != expected_message {
            bail!("Config inspector option inventory diagnostic message is invalid");
        }
        let encoded_path = serde_json::to_vec(&diagnostic.path)?;
        if previous.as_ref().is_some_and(|path| path >= &encoded_path) {
            bail!("Config inspector option inventory diagnostics are not ordered uniquely");
        }
        previous = Some(encoded_path);
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

fn canonical_provenance_digest(
    adapter_version: u64,
    target_lib_version: &Option<String>,
    target_module_system_path: &Option<String>,
    definitions_by_option: &[RawDefinitionsForOption],
) -> Result<String> {
    let definitions_by_option = definitions_by_option
        .iter()
        .map(|option| CanonicalDefinitionsForOption {
            definitions: option
                .definitions
                .iter()
                .map(|definition| CanonicalRawDefinition {
                    module_key: &definition.module_key,
                    ordinal: definition.ordinal,
                    priority: definition.priority,
                    source_input: &definition.source_input,
                    source_path: &definition.source_path,
                    source_revision: &definition.source_revision,
                    status: definition.status,
                    surviving_merge_order: definition.surviving_merge_order,
                })
                .collect(),
            option_key: &option.option_key,
            path: &option.path,
        })
        .collect();
    let canonical = CanonicalProvenance {
        definitions_by_option,
        provenance_adapter_version: adapter_version,
        target_lib_version,
        target_module_system_path,
    };
    Ok(format_digest(&serde_json::to_vec(&canonical)?))
}

fn reconcile_provenance(payload: ProvenancePayload, index: &IndexPayload) -> InspectionProvenance {
    if payload.adapter_version != EXPECTED_PROVENANCE_ADAPTER_VERSION {
        return unavailable_provenance("unsupported_adapter_version", None);
    }
    if !payload.supported {
        return unavailable_provenance(
            sanitize_adapter_reason(payload.reason_code.as_deref()),
            None,
        );
    }

    let Some(mut provenance_digest) = payload.provenance_digest.filter(|digest| is_digest(digest))
    else {
        return unavailable_provenance("malformed_payload", None);
    };

    let Some(definitions_by_option_payload) = payload.definitions_by_option else {
        return unavailable_provenance("malformed_payload", None);
    };

    let index_paths = index
        .options
        .iter()
        .map(|entry| (entry.key.as_str(), entry.path.as_slice()))
        .collect::<HashMap<_, _>>();
    let mut seen_options = HashSet::new();
    let mut definitions_by_option = Vec::with_capacity(definitions_by_option_payload.len());

    for option in definitions_by_option_payload {
        let indexed_path = index_paths.get(option.option_key.as_str());
        if indexed_path.is_none() {
            if index.option_inventory_complete {
                return unavailable_provenance("provenance_integrity_failure", None);
            }
            // INVARIANT: The exact Stage-1 index is the completeness authority.
            // Diagnostic paths are bounded and can be redacted, so they cannot
            // identify every omitted provenance group in a partial inventory.
            continue;
        }
        if !seen_options.insert(option.option_key.clone())
            || option_key(&option.path) != option.option_key
            || indexed_path.is_none_or(|path| *path != option.path.as_slice())
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
        if !(0..definitions.len() as u64).all(|ordinal| ordinals.contains(&ordinal)) {
            return unavailable_provenance("provenance_integrity_failure", None);
        }
        let surviving_count = definitions
            .iter()
            .filter(|definition| definition.status == RawDefinitionStatus::ActiveSurviving)
            .count();
        if !(0..surviving_count as u64).all(|order| merge_orders.contains(&order)) {
            return unavailable_provenance("provenance_integrity_failure", None);
        }
        definitions_by_option.push(RawDefinitionsForOption {
            option_key: option.option_key,
            path: option.path,
            definitions,
        });
    }

    if !index.option_inventory_complete {
        provenance_digest = match canonical_provenance_digest(
            payload.adapter_version,
            &payload.target_lib_version,
            &payload.target_module_system_path,
            &definitions_by_option,
        ) {
            Ok(digest) => digest,
            Err(_) => return unavailable_provenance("provenance_integrity_failure", None),
        };
    }

    InspectionProvenance::Available {
        adapter_version: payload.adapter_version,
        target_lib_version: payload.target_lib_version,
        target_module_system_path: payload.target_module_system_path,
        provenance_digest,
        definitions_by_option,
    }
}

fn sanitize_adapter_reason(reason: Option<&str>) -> &'static str {
    match reason {
        Some("module_type_payload_unavailable") => "module_type_payload_unavailable",
        Some("helper_capability_unavailable") => "helper_capability_unavailable",
        Some("capability_self_test_failed") => "capability_self_test_failed",
        Some("graph_replay_mismatch") => "graph_replay_mismatch",
        Some("adapter_evaluation_failed") => "adapter_evaluation_failed",
        Some("provenance_integrity_failure") => "provenance_integrity_failure",
        Some("adapter_unsupported") => "adapter_unsupported",
        _ => "adapter_unsupported",
    }
}

fn validate_key(key: &str) -> Result<()> {
    if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid Config inspector option hash {key}");
    }
    Ok(())
}

fn validate_source_out_path(path: &str) -> Result<()> {
    if path.is_empty() || !path.starts_with("/nix/store/") {
        bail!("invalid resolved flake source outPath");
    }
    Ok(())
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Holds the validated value results from the separate definition-value jobset.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DefinitionValueEnrichment {
    /// Every Stage-1 definition has one validated Stage-2 result.
    Available {
        /// Adapter protocol version that produced the index.
        adapter_version: u64,
        /// Digest binding the Stage-2 index to Stage-1 provenance.
        provenance_digest: String,
        /// Values in deterministic option-key/ordinal order.
        values: Vec<DefinitionValue>,
    },
    /// Stage-2 could not be correlated with Stage-1 provenance.
    Unavailable {
        /// Stable reason for the unavailable enrichment.
        reason_code: String,
        /// Sanitized diagnostic, when one is available.
        diagnostic: Option<SafeEvaluationError>,
    },
}

/// Represents one raw definition's independently evaluated semantic value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DefinitionValue {
    /// Collision-resistant option identity.
    pub option_key: String,
    /// Definition ordinal within the option's provenance index.
    pub ordinal: u64,
    /// Available safe value or an isolated evaluation failure.
    pub value: InspectionValue,
}

/// Contains the truthful semantic join of both validated inspector stages.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AssembledConfigInspection {
    /// Immutable target identity shared by both stages.
    pub target_key: String,
    /// Resolved immutable flake source path.
    pub source_out_path: String,
    /// Shared carrier derivation path.
    pub carrier_drv_path: String,
    /// Whether Stage 1 enumerated the complete option tree.
    pub option_inventory_complete: bool,
    /// Bounded unreadable option-tree prefixes.
    pub option_inventory_diagnostics: Vec<OptionInventoryDiagnostic>,
    /// Whether the bounded diagnostic vector omits additional unreadable prefixes.
    pub option_inventory_diagnostics_truncated: bool,
    /// Configuration-global provenance capture and enrichment state.
    pub provenance_state: AssembledConfigProvenanceState,
    /// Options in Stage-1 index order.
    pub options: Vec<AssembledOption>,
}

/// Describes configuration-global provenance and definition-value state.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AssembledConfigProvenanceState {
    /// Provenance metadata and global enrichment state are available.
    Available {
        /// Adapter protocol version that produced the definitions.
        adapter_version: u64,
        /// Target library version, when available.
        target_lib_version: Option<String>,
        /// Target module-system source path, when available.
        target_module_system_path: Option<String>,
        /// Canonical raw-definition digest.
        provenance_digest: String,
        /// Global state of the Stage-2 definition-value layer.
        definition_value_enrichment: DefinitionValueEnrichmentState,
    },
    /// Configuration-wide raw provenance was not established.
    Unavailable {
        /// Stable reason for the unavailable state.
        reason_code: String,
        /// Sanitized diagnostic, when available.
        diagnostic: Option<SafeEvaluationError>,
    },
}

/// Contains one option with its validated metadata, value, and provenance.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AssembledOption {
    /// Collision-resistant option identity.
    pub option_key: String,
    /// Exact option path components.
    pub path_components: Vec<String>,
    /// Metadata result preserved without defaulting failures.
    pub metadata: InspectionMetadata,
    /// Effective value result preserved without defaulting failures.
    pub effective_value: InspectionValue,
    /// Explicitly available or unavailable raw-definition provenance.
    pub provenance: AssembledOptionProvenance,
}

/// Describes whether raw-definition provenance is available for an option.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AssembledOptionProvenance {
    /// Raw definitions and local override state are known.
    Available {
        /// All definitions retained in ordinal order.
        definitions: Vec<AssembledDefinition>,
        /// Whether priority-discarded definitions were observed.
        override_state: OverrideState,
    },
    /// Raw-definition provenance was not established for this option.
    Unavailable,
}

/// Represents whether an available option has discarded definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverrideState {
    /// The available provenance establishes the Boolean value.
    Known(bool),
}

/// Contains one raw definition and its optional Stage-2 value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AssembledDefinition {
    /// Option identity repeated for unambiguous downstream joins.
    pub option_key: String,
    /// Definition identity within the option.
    pub ordinal: u64,
    /// Source module path, when supplied by the module system.
    pub source_path: Option<String>,
    /// Flake input name, when resolved.
    pub source_input: Option<String>,
    /// Full source revision, when resolved.
    pub source_revision: Option<String>,
    /// Module-system key, when supplied.
    pub module_key: Option<String>,
    /// Module-system priority.
    pub priority: i64,
    /// Raw definition status.
    pub status: RawDefinitionStatus,
    /// Surviving-definition merge order, when applicable.
    pub surviving_merge_order: Option<u64>,
    /// Definition value, or `None` when the global Stage-2 layer is unavailable.
    pub value: Option<InspectionValue>,
}

/// Describes the global state of definition-value enrichment.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DefinitionValueEnrichmentState {
    /// Every definition has one explicit available or failed value.
    Available {
        /// Adapter protocol version bound to the values.
        adapter_version: u64,
        /// Provenance digest bound to the values.
        provenance_digest: String,
    },
    /// No per-definition value was established by Stage 2.
    Unavailable {
        /// Stable reason for the unavailable enrichment.
        reason_code: String,
        /// Sanitized diagnostic, when available.
        diagnostic: Option<SafeEvaluationError>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
struct DefinitionIdentity {
    option_key: String,
    ordinal: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct DefinitionIndexPayload {
    kind: String,
    #[serde(rename = "targetKey")]
    target_key: String,
    #[serde(rename = "sourceOutPath")]
    source_out_path: String,
    #[serde(rename = "adapterVersion")]
    adapter_version: u64,
    supported: bool,
    #[serde(rename = "reasonCode")]
    reason_code: Option<String>,
    #[serde(rename = "provenanceDigest")]
    provenance_digest: Option<String>,
    #[serde(rename = "definitionCount")]
    definition_count: usize,
    definitions: Vec<DefinitionIdentityPayload>,
}

#[derive(Debug, Clone, Deserialize)]
struct DefinitionIdentityPayload {
    option_key: String,
    ordinal: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct DefinitionValuePayload {
    kind: String,
    option_key: String,
    ordinal: u64,
    value: SafeOptionValue,
}

/// Reconciles the separate Stage-2 JSONL output against validated Stage-1
/// provenance.
///
/// The index, digest, adapter version, exact identity set, complete result set,
/// and shared carrier derivation are all checked before values become available.
/// Per-definition evaluator errors remain isolated as `not_evaluated` values.
/// Missing, duplicate, unknown, or malformed results reject the complete
/// enrichment instead of fabricating a value.
pub(crate) fn reconcile_definition_values_output(
    output: &[u8],
    expected_stage1: &ConfigInspectorResult,
) -> Result<DefinitionValueEnrichment> {
    let expected_provenance = &expected_stage1.provenance;
    let InspectionProvenance::Available {
        adapter_version: expected_adapter_version,
        provenance_digest: expected_digest,
        definitions_by_option,
        ..
    } = expected_provenance
    else {
        return Ok(DefinitionValueEnrichment::Unavailable {
            reason_code: "stage1_provenance_unavailable".to_string(),
            diagnostic: None,
        });
    };

    let expected = definitions_by_option
        .iter()
        .flat_map(|option| {
            option
                .definitions
                .iter()
                .map(move |definition| DefinitionIdentity {
                    option_key: option.option_key.clone(),
                    ordinal: definition.ordinal,
                })
        })
        .collect::<std::collections::BTreeSet<_>>();

    if expected.len()
        != definitions_by_option
            .iter()
            .map(|option| option.definitions.len())
            .sum::<usize>()
    {
        return Ok(DefinitionValueEnrichment::Unavailable {
            reason_code: "provenance_identity_mismatch".to_string(),
            diagnostic: None,
        });
    }

    let mut index: Option<DefinitionIndexPayload> = None;
    let mut values = HashMap::<DefinitionIdentity, JobResult>::new();
    let mut seen_attributes = HashSet::new();
    let mut carrier_drv_path = None;

    for (line_number, line) in output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .enumerate()
    {
        let result: JobResult = serde_json::from_slice(line).with_context(|| {
            format!("invalid definition-value JSONL at line {}", line_number + 1)
        })?;
        if !seen_attributes.insert(result.attr.clone()) {
            bail!("duplicate definition-value result for {}", result.attr);
        }

        let is_index = result.attr == DEFINITION_INDEX_ATTRIBUTE;
        let identity = if is_index {
            None
        } else if result.attr.starts_with(DEFINITION_VALUE_PREFIX) {
            Some(parse_definition_job_name(&result.attr)?)
        } else {
            bail!("unknown Stage-2 job attribute {}", result.attr);
        };

        if result.error.is_none() {
            let drv_path = result.drv_path.as_ref().ok_or_else(|| {
                anyhow::anyhow!("successful Stage-2 result {} has no drvPath", result.attr)
            })?;
            if let Some(expected_drv_path) = &carrier_drv_path {
                if expected_drv_path != drv_path {
                    bail!("Stage-2 successful drvPath changed");
                }
            } else {
                carrier_drv_path = Some(drv_path.clone());
            }
        }

        if let Some(error) = result.error.as_deref() {
            if is_index {
                return Ok(DefinitionValueEnrichment::Unavailable {
                    reason_code: "stage2_index_failed".to_string(),
                    diagnostic: Some(failed_evaluation("stage2_index_failed", &error)),
                });
            }
            values.insert(identity.expect("definition identity exists"), result);
            continue;
        }

        let payload = result.extra_value.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "successful Stage-2 result {} has no extraValue",
                result.attr
            )
        })?;
        if is_index {
            if index.is_some() {
                bail!("duplicate Stage-2 definition index");
            }
            index = Some(serde_json::from_value(payload).context("invalid Stage-2 index payload")?);
        } else {
            values.insert(identity.expect("definition identity exists"), result);
        }
    }

    let index = index.ok_or_else(|| anyhow::anyhow!("Stage-2 definition index is missing"))?;
    if index.kind != "definition_index" {
        bail!("invalid Stage-2 definition index kind");
    }
    if !is_digest(&index.target_key) {
        bail!("Stage-2 definition index target key is malformed");
    }
    if index.target_key != expected_stage1.target_key {
        return Ok(DefinitionValueEnrichment::Unavailable {
            reason_code: "inspection_target_mismatch".to_string(),
            diagnostic: None,
        });
    }
    if !validate_source_out_path(&index.source_out_path).is_ok() {
        bail!("Stage-2 definition index source path is malformed");
    }
    if index.source_out_path != expected_stage1.source_out_path {
        return Ok(DefinitionValueEnrichment::Unavailable {
            reason_code: "flake_source_mismatch".to_string(),
            diagnostic: None,
        });
    }
    if carrier_drv_path.as_deref() != Some(expected_stage1.carrier_drv_path.as_str()) {
        return Ok(DefinitionValueEnrichment::Unavailable {
            reason_code: "carrier_derivation_mismatch".to_string(),
            diagnostic: None,
        });
    }
    if !index.supported {
        if !index.definitions.is_empty() || !values.is_empty() || index.definition_count != 0 {
            bail!("unsupported Stage-2 index contains definition jobs");
        }
        return Ok(DefinitionValueEnrichment::Unavailable {
            reason_code: sanitize_adapter_reason(index.reason_code.as_deref()).to_string(),
            diagnostic: None,
        });
    }
    let Some(stage2_digest) = index.provenance_digest.as_deref() else {
        bail!("Stage-2 definition index digest is missing");
    };
    if !is_digest(stage2_digest) {
        bail!("Stage-2 definition index digest is malformed");
    }
    if index.adapter_version != *expected_adapter_version {
        return Ok(DefinitionValueEnrichment::Unavailable {
            reason_code: "provenance_adapter_version_mismatch".to_string(),
            diagnostic: None,
        });
    }
    if stage2_digest != expected_digest {
        return Ok(DefinitionValueEnrichment::Unavailable {
            reason_code: "provenance_digest_mismatch".to_string(),
            diagnostic: None,
        });
    }
    if index.definition_count != index.definitions.len() {
        bail!("Stage-2 definition count does not match index length");
    }

    let mut indexed = std::collections::BTreeSet::new();
    for definition in index.definitions {
        validate_key(&definition.option_key)?;
        let identity = DefinitionIdentity {
            option_key: definition.option_key,
            ordinal: definition.ordinal,
        };
        if !indexed.insert(identity) {
            bail!("duplicate Stage-2 definition identity");
        }
    }
    if index.definition_count != indexed.len() {
        bail!("Stage-2 definition count does not match unique identities");
    }
    if indexed != expected {
        return Ok(DefinitionValueEnrichment::Unavailable {
            reason_code: "provenance_identity_mismatch".to_string(),
            diagnostic: None,
        });
    }
    if values.keys().any(|identity| !expected.contains(identity)) {
        bail!("unknown Stage-2 definition result");
    }
    if values.len() != expected.len() {
        bail!("missing expected Stage-2 definition result");
    }

    let values = expected
        .into_iter()
        .map(|identity| {
            let result = values
                .remove(&identity)
                .expect("validated Stage-2 identity has a result");
            let value = if let Some(error) = result.error {
                InspectionValue::Failed(failed_evaluation("not_evaluated", &error))
            } else {
                let payload: DefinitionValuePayload = serde_json::from_value(
                    result
                        .extra_value
                        .expect("successful result has extraValue"),
                )
                .with_context(|| format!("invalid Stage-2 value payload for {}", result.attr))?;
                if payload.kind != "definition_value"
                    || payload.option_key != identity.option_key
                    || payload.ordinal != identity.ordinal
                {
                    bail!("Stage-2 value payload does not match {}", result.attr);
                }
                InspectionValue::Available(payload.value)
            };
            Ok(DefinitionValue {
                option_key: identity.option_key,
                ordinal: identity.ordinal,
                value,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(DefinitionValueEnrichment::Available {
        adapter_version: *expected_adapter_version,
        provenance_digest: expected_digest.clone(),
        values,
    })
}

/// Joins validated Stage-1 options with validated Stage-2 definition values.
///
/// This function performs no Nix evaluation, JSONL parsing, hashing, or
/// provenance reconstruction. It preserves unavailable and failed states
/// instead of coercing them into empty or fabricated durable values.
pub(crate) fn assemble_config_inspection(
    stage1: ConfigInspectorResult,
    stage2: DefinitionValueEnrichment,
) -> Result<AssembledConfigInspection> {
    let ConfigInspectorResult {
        target_key,
        carrier_drv_path,
        source_out_path,
        option_inventory_complete,
        option_inventory_diagnostics,
        option_inventory_diagnostics_truncated,
        options,
        provenance,
    } = stage1;

    let stage2_state = match stage2 {
        DefinitionValueEnrichment::Available {
            adapter_version,
            provenance_digest,
            values,
        } => Stage2AssemblyInput::Available {
            adapter_version,
            provenance_digest,
            values,
        },
        DefinitionValueEnrichment::Unavailable {
            reason_code,
            diagnostic,
        } => Stage2AssemblyInput::Unavailable {
            reason_code,
            diagnostic,
        },
    };

    let (provenance_state, assembled_options) = match provenance {
        InspectionProvenance::Unavailable {
            reason_code,
            diagnostic,
        } => {
            if !matches!(
                &stage2_state,
                Stage2AssemblyInput::Unavailable { reason_code: stage2_reason, .. }
                    if stage2_reason == "stage1_provenance_unavailable"
            ) {
                bail!(
                    "Stage-2 enrichment is available or has an incompatible reason while Stage-1 provenance is unavailable"
                );
            }
            let provenance_state = AssembledConfigProvenanceState::Unavailable {
                reason_code,
                diagnostic,
            };
            let assembled_options = options
                .into_iter()
                .map(|option| AssembledOption {
                    option_key: option.key,
                    path_components: option.path_components,
                    metadata: option.metadata,
                    effective_value: option.value,
                    provenance: AssembledOptionProvenance::Unavailable,
                })
                .collect();
            (provenance_state, assembled_options)
        }
        InspectionProvenance::Available {
            adapter_version,
            target_lib_version,
            target_module_system_path,
            provenance_digest,
            definitions_by_option,
        } => {
            let (definition_value_enrichment, definition_values) = match stage2_state {
                Stage2AssemblyInput::Unavailable {
                    reason_code,
                    diagnostic,
                } => {
                    if reason_code == "stage1_provenance_unavailable" {
                        bail!(
                            "Stage-2 reports unavailable provenance while Stage-1 provenance is available"
                        );
                    }
                    (
                        DefinitionValueEnrichmentState::Unavailable {
                            reason_code,
                            diagnostic,
                        },
                        BTreeMap::new(),
                    )
                }
                Stage2AssemblyInput::Available {
                    adapter_version: stage2_adapter_version,
                    provenance_digest: stage2_provenance_digest,
                    values,
                } => {
                    if stage2_adapter_version != adapter_version {
                        bail!("Stage-1 and Stage-2 provenance adapter versions disagree");
                    }
                    if stage2_provenance_digest != provenance_digest {
                        bail!("Stage-1 and Stage-2 provenance digests disagree");
                    }
                    (
                        DefinitionValueEnrichmentState::Available {
                            adapter_version: stage2_adapter_version,
                            provenance_digest: stage2_provenance_digest,
                        },
                        definition_value_map(values)?,
                    )
                }
            };

            let mut option_paths = BTreeMap::new();
            for option in &options {
                if option_paths
                    .insert(option.key.clone(), option.path_components.clone())
                    .is_some()
                {
                    bail!("duplicate Stage-1 option key {}", option.key);
                }
            }
            let provenance_groups = provenance_group_map(definitions_by_option, &option_paths)?;
            if matches!(
                &definition_value_enrichment,
                DefinitionValueEnrichmentState::Available { .. }
            ) {
                let expected_definition_values = provenance_groups
                    .values()
                    .flat_map(|group| {
                        group
                            .definitions
                            .iter()
                            .map(|definition| DefinitionIdentity {
                                option_key: group.option_key.clone(),
                                ordinal: definition.ordinal,
                            })
                    })
                    .collect::<BTreeSet<_>>();
                let actual_definition_values =
                    definition_values.keys().cloned().collect::<BTreeSet<_>>();
                if actual_definition_values != expected_definition_values {
                    bail!("Stage-2 definition identities disagree during assembly");
                }
            }

            let provenance_state = AssembledConfigProvenanceState::Available {
                adapter_version,
                target_lib_version,
                target_module_system_path,
                provenance_digest,
                definition_value_enrichment: definition_value_enrichment.clone(),
            };
            let assembled_options = options
                .into_iter()
                .map(|option| {
                    // `buildRawDefinitionsByOption` groups only options with
                    // at least one raw definition. An omitted group therefore
                    // means known zero definitions, not unavailable provenance.
                    let definitions = provenance_groups
                        .get(&option.key)
                        .map(|group| {
                            assemble_definitions(
                                &option.key,
                                group,
                                &definition_value_enrichment,
                                &definition_values,
                            )
                        })
                        .transpose()?
                        .unwrap_or_default();
                    let override_state =
                        OverrideState::Known(definitions.iter().any(|definition| {
                            definition.status == RawDefinitionStatus::PriorityDiscarded
                        }));
                    Ok(AssembledOption {
                        option_key: option.key,
                        path_components: option.path_components,
                        metadata: option.metadata,
                        effective_value: option.value,
                        provenance: AssembledOptionProvenance::Available {
                            definitions,
                            override_state,
                        },
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            (provenance_state, assembled_options)
        }
    };

    Ok(AssembledConfigInspection {
        target_key,
        source_out_path,
        carrier_drv_path,
        option_inventory_complete,
        option_inventory_diagnostics,
        option_inventory_diagnostics_truncated,
        provenance_state,
        options: assembled_options,
    })
}

enum Stage2AssemblyInput {
    Available {
        adapter_version: u64,
        provenance_digest: String,
        values: Vec<DefinitionValue>,
    },
    Unavailable {
        reason_code: String,
        diagnostic: Option<SafeEvaluationError>,
    },
}

fn definition_value_map(
    values: Vec<DefinitionValue>,
) -> Result<BTreeMap<DefinitionIdentity, InspectionValue>> {
    let mut map = BTreeMap::new();
    for value in values {
        let identity = DefinitionIdentity {
            option_key: value.option_key,
            ordinal: value.ordinal,
        };
        if map.insert(identity, value.value).is_some() {
            bail!("duplicate Stage-2 definition value identity");
        }
    }
    Ok(map)
}

fn provenance_group_map(
    groups: Vec<RawDefinitionsForOption>,
    option_paths: &BTreeMap<String, Vec<String>>,
) -> Result<BTreeMap<String, RawDefinitionsForOption>> {
    let mut map = BTreeMap::new();
    for group in groups {
        let Some(expected_path) = option_paths.get(&group.option_key) else {
            bail!("unknown provenance option key {}", group.option_key);
        };
        if expected_path != &group.path {
            bail!("provenance path does not match option {}", group.option_key);
        }
        if map.insert(group.option_key.clone(), group).is_some() {
            bail!("duplicate provenance option group");
        }
    }
    Ok(map)
}

fn assemble_definitions(
    option_key: &str,
    group: &RawDefinitionsForOption,
    enrichment: &DefinitionValueEnrichmentState,
    values: &BTreeMap<DefinitionIdentity, InspectionValue>,
) -> Result<Vec<AssembledDefinition>> {
    let mut definitions = group.definitions.clone();
    definitions.sort_by_key(|definition| definition.ordinal);
    let mut ordinals = BTreeSet::new();
    let mut surviving_merge_orders = BTreeSet::new();
    let mut survivor_count = 0_u64;
    // INVARIANT: Stage-1 emits a complete zero-based ordinal permutation, and
    // survivor merge order is a separate complete zero-based permutation.
    for (expected_ordinal, definition) in definitions.iter().enumerate() {
        let expected_ordinal = u64::try_from(expected_ordinal)
            .context("raw definition count exceeds supported ordinal range")?;
        if definition.ordinal != expected_ordinal {
            bail!(
                "raw definition ordinals are not a zero-based permutation for option {option_key}"
            );
        }
        if !ordinals.insert(definition.ordinal) {
            bail!("duplicate raw definition ordinal for option {option_key}");
        }
        match definition.status {
            RawDefinitionStatus::ActiveSurviving => {
                let Some(merge_order) = definition.surviving_merge_order else {
                    bail!(
                        "active surviving definition is missing merge order for option {option_key}"
                    );
                };
                if !surviving_merge_orders.insert(merge_order) {
                    bail!("duplicate surviving merge order for option {option_key}");
                }
                survivor_count += 1;
            }
            RawDefinitionStatus::PriorityDiscarded => {
                if definition.surviving_merge_order.is_some() {
                    bail!("discarded definition has surviving merge order for option {option_key}");
                }
            }
        }
    }
    let expected_merge_orders = (0..survivor_count).collect::<BTreeSet<_>>();
    if surviving_merge_orders != expected_merge_orders {
        bail!("surviving merge orders are not a zero-based permutation for option {option_key}");
    }

    definitions
        .into_iter()
        .map(|definition| {
            let identity = DefinitionIdentity {
                option_key: option_key.to_string(),
                ordinal: definition.ordinal,
            };
            let value = match enrichment {
                DefinitionValueEnrichmentState::Available { .. } => Some(
                    values
                        .get(&identity)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("missing Stage-2 value for definition"))?,
                ),
                DefinitionValueEnrichmentState::Unavailable { .. } => None,
            };
            Ok(AssembledDefinition {
                option_key: option_key.to_string(),
                ordinal: definition.ordinal,
                source_path: definition.source_path,
                source_input: definition.source_input,
                source_revision: definition.source_revision,
                module_key: definition.module_key,
                priority: definition.priority,
                status: definition.status,
                surviving_merge_order: definition.surviving_merge_order,
                value,
            })
        })
        .collect()
}

fn parse_definition_job_name(attribute: &str) -> Result<DefinitionIdentity> {
    let suffix = attribute
        .strip_prefix(DEFINITION_VALUE_PREFIX)
        .ok_or_else(|| anyhow::anyhow!("invalid Stage-2 definition job name"))?;
    if suffix.len() <= 65 || suffix.as_bytes()[64] != b'_' {
        bail!("malformed Stage-2 definition job name {attribute}");
    }
    let option_key = &suffix[..64];
    validate_key(option_key)?;
    let ordinal_text = &suffix[65..];
    let ordinal = ordinal_text
        .parse::<u64>()
        .with_context(|| format!("malformed Stage-2 definition ordinal {ordinal_text}"))?;
    if ordinal.to_string() != ordinal_text {
        bail!("non-canonical Stage-2 definition ordinal {ordinal_text}");
    }
    Ok(DefinitionIdentity {
        option_key: option_key.to_string(),
        ordinal,
    })
}

/// Returns the canonical SHA-256 identity for exact option path components.
pub(crate) fn option_key(path: &[String]) -> String {
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

    const TEST_TARGET_KEY: &str =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TEST_SOURCE_OUT_PATH: &str = "/nix/store/test-flake-source";
    const TEST_DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

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

    fn test_target() -> InspectionTarget {
        InspectionTarget {
            flake_ref: "test-flake".to_string(),
            configuration_name: "good".to_string(),
            target_key: TEST_TARGET_KEY.to_string(),
        }
    }

    fn semantic_metadata(path: &[&str]) -> OptionMetadata {
        OptionMetadata {
            path: path.iter().map(|part| (*part).to_string()).collect(),
            option_type: Some("option".to_string()),
            loc: Vec::new(),
            declared_type: Some("boolean".to_string()),
            declarations: Vec::new(),
            declaration_positions: Vec::new(),
            highest_prio: Some(100),
            is_defined: true,
            surviving_definition_sources: Vec::new(),
        }
    }

    fn semantic_option(
        key: &str,
        path: &[&str],
        metadata: InspectionMetadata,
        value: InspectionValue,
    ) -> InspectedOption {
        InspectedOption {
            path_components: path.iter().map(|part| (*part).to_string()).collect(),
            key: key.to_string(),
            metadata,
            value,
        }
    }

    fn semantic_stage1(
        options: Vec<InspectedOption>,
        provenance: InspectionProvenance,
    ) -> ConfigInspectorResult {
        ConfigInspectorResult {
            target_key: TEST_TARGET_KEY.to_string(),
            carrier_drv_path: "/nix/store/shared.drv".to_string(),
            source_out_path: TEST_SOURCE_OUT_PATH.to_string(),
            option_inventory_complete: true,
            option_inventory_diagnostics: Vec::new(),
            option_inventory_diagnostics_truncated: false,
            options,
            provenance,
        }
    }

    fn semantic_definition(
        ordinal: u64,
        priority: i64,
        status: RawDefinitionStatus,
        source_path: Option<&str>,
        surviving_merge_order: Option<u64>,
    ) -> RawDefinition {
        RawDefinition {
            source_path: source_path.map(str::to_string),
            source_input: Some("self".to_string()),
            source_revision: Some("rev".to_string()),
            module_key: Some(format!("module-{ordinal}")),
            ordinal,
            priority,
            status,
            surviving_merge_order,
        }
    }

    fn semantic_provenance(groups: Vec<RawDefinitionsForOption>) -> InspectionProvenance {
        InspectionProvenance::Available {
            adapter_version: EXPECTED_PROVENANCE_ADAPTER_VERSION,
            target_lib_version: Some("lib".to_string()),
            target_module_system_path: Some("/nix/store/lib".to_string()),
            provenance_digest: TEST_DIGEST.to_string(),
            definitions_by_option: groups,
        }
    }

    fn semantic_value(value: bool) -> InspectionValue {
        InspectionValue::Available(SafeOptionValue::Scalar(json!(value)))
    }

    fn semantic_stage2(values: Vec<DefinitionValue>) -> DefinitionValueEnrichment {
        DefinitionValueEnrichment::Available {
            adapter_version: EXPECTED_PROVENANCE_ADAPTER_VERSION,
            provenance_digest: TEST_DIGEST.to_string(),
            values,
        }
    }

    fn semantic_definition_value(
        option_key: &str,
        ordinal: u64,
        value: InspectionValue,
    ) -> DefinitionValue {
        DefinitionValue {
            option_key: option_key.to_string(),
            ordinal,
            value,
        }
    }

    fn unreadable_diagnostic(path: &str) -> OptionInventoryDiagnostic {
        OptionInventoryDiagnostic {
            path: vec![path.to_string()],
            code: "unreadable_option_subtree".to_string(),
            message: "Option subtree could not be inspected".to_string(),
        }
    }

    #[test]
    fn option_inventory_validation_enforces_bounded_partial_state_and_byte_order() {
        assert!(validate_option_inventory(true, &[], false).is_ok());
        assert!(
            validate_option_inventory(false, &[unreadable_diagnostic("poison")], false).is_ok()
        );
        assert!(
            validate_option_inventory(false, &[unreadable_diagnostic("[REDACTED]")], true).is_ok()
        );

        let raw_risky = [
            unreadable_diagnostic(""),
            unreadable_diagnostic(&"x".repeat(MAX_OPTION_INVENTORY_PATH_COMPONENT_CHARS + 1)),
        ];
        assert!(validate_raw_option_inventory(false, &raw_risky, false).is_ok());
        assert!(validate_option_inventory(false, &raw_risky, false).is_err());

        let bounded = (0..MAX_OPTION_INVENTORY_DIAGNOSTICS)
            .map(|index| unreadable_diagnostic(&format!("poison{index:03}")))
            .collect::<Vec<_>>();
        assert!(validate_option_inventory(false, &bounded, true).is_ok());

        let mut over_limit = bounded.clone();
        over_limit.push(unreadable_diagnostic("poison128"));
        assert!(validate_option_inventory(false, &over_limit, true).is_err());
        assert!(validate_option_inventory(false, &[], false).is_err());
        assert!(validate_option_inventory(true, &bounded, true).is_err());

        let byte_ordered = [
            unreadable_diagnostic("Zoo"),
            unreadable_diagnostic("alpha"),
            unreadable_diagnostic("\u{c5}ngstr\u{f6}m"),
        ];
        assert!(validate_option_inventory(false, &byte_ordered, false).is_ok());
        let mut reversed = byte_ordered;
        reversed.reverse();
        assert!(validate_option_inventory(false, &reversed, false).is_err());
    }

    fn reconcile_for_test(output: &str) -> Result<ConfigInspectorResult> {
        if output.contains(PROVENANCE_ATTRIBUTE) {
            reconcile_inspector_output(output.as_bytes(), &test_target())
        } else {
            let provenance = result(
                PROVENANCE_ATTRIBUTE,
                json!({
                    "adapterVersion": EXPECTED_PROVENANCE_ADAPTER_VERSION,
                    "supported": false,
                    "reasonCode": "capability_self_test_failed",
                }),
            );
            reconcile_inspector_output(format!("{output}\n{provenance}").as_bytes(), &test_target())
        }
    }

    fn index(key: &str, path: &[&str]) -> Value {
        json!({
            "kind": "index",
            "targetKey": TEST_TARGET_KEY,
            "sourceOutPath": TEST_SOURCE_OUT_PATH,
            "options": [{
                "key": key,
                "path": path,
            }],
            "optionInventoryComplete": true,
            "optionInventoryDiagnostics": [],
            "optionInventoryDiagnosticsTruncated": false,
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
        let target = InspectionTarget::new("path:/tmp/example", "good");
        let expression = build_inspector_expression(&target);

        assert!(
            expression.contains("builtins.getAttr configurationName flake.nixosConfigurations")
        );
        assert!(expression.contains("configuration.pkgs.lib"));
        assert_eq!(expression.matches("builtins.getFlake").count(), 1);
        assert_eq!(
            build_definition_values_expression(&target, "/tmp/allowed-options.json")
                .matches("builtins.getFlake")
                .count(),
            1
        );
        assert!(
            build_definition_values_expression(&target, "/tmp/allowed-options.json")
                .contains("builtins.fromJSON (builtins.readFile")
        );
        assert!(
            build_definition_values_expression(&target, "/tmp/allowed-options.json")
                .contains("collectModulesResultSupported")
        );
        let inspector_source = include_str!("config_inspector.nix");
        assert_eq!(inspector_source.matches("builtins.getFlake").count(), 0);
        assert!(!inspector_source.contains("configurationName"));
        assert!(!inspector_source.contains("flake.nixosConfigurations"));
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
        let expression = build_inspector_expression(&InspectionTarget::new(value, value));

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
        let expression =
            build_inspector_expression(&InspectionTarget::new("before\0after", "good"));

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
                    "targetKey": TEST_TARGET_KEY,
                    "sourceOutPath": TEST_SOURCE_OUT_PATH,
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
                    "targetKey": TEST_TARGET_KEY,
                    "sourceOutPath": TEST_SOURCE_OUT_PATH,
                    "options": [
                        { "key": dotted_key, "path": dotted },
                        { "key": dotted_component_key, "path": dotted_component },
                        { "key": special_key, "path": special },
                    ],
                    "optionInventoryComplete": true,
                    "optionInventoryDiagnostics": [],
                    "optionInventoryDiagnosticsTruncated": false,
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
            "provenanceDigest": TEST_DIGEST,
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

    fn output_with_provenance(path: &[&str], key: &str, provenance: Value) -> String {
        [
            result("__crystalForgeConfigIndex", index(key, path)),
            result(&format!("meta_{key}"), metadata(key, path)),
            result(
                &format!("value_{key}"),
                json!({
                    "kind": "value",
                    "key": key,
                    "value": { "kind": "scalar", "value": true },
                }),
            ),
            result(PROVENANCE_ATTRIBUTE, provenance),
        ]
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
    }

    fn stage1_context() -> ConfigInspectorResult {
        let path = ["crystalForgeProbe", "target"];
        let key = hash(&path);
        let output = output_with_provenance(&path, &key, supported_provenance(&path, &key));
        reconcile_inspector_output(output.as_bytes(), &test_target())
            .expect("Stage-1 fixture is valid")
    }

    fn stage2_output_with_digest(
        target_key: &str,
        drv_path: &str,
        provenance_digest: &str,
        identities: &[(&str, u64)],
    ) -> String {
        let index = json!({
            "kind": "definition_index",
            "targetKey": target_key,
            "sourceOutPath": TEST_SOURCE_OUT_PATH,
            "adapterVersion": EXPECTED_PROVENANCE_ADAPTER_VERSION,
            "supported": true,
            "provenanceDigest": provenance_digest,
            "definitionCount": identities.len(),
            "definitions": identities.iter().map(|(option_key, ordinal)| json!({
                "option_key": option_key,
                "ordinal": ordinal,
            })).collect::<Vec<_>>(),
        });
        let mut lines = vec![result("__crystalForgeDefinitionIndex", index)];
        for (option_key, ordinal) in identities {
            let attr = format!("def_value_{option_key}_{ordinal}");
            lines.push(result(
                &attr,
                json!({
                    "kind": "definition_value",
                    "option_key": option_key,
                    "ordinal": ordinal,
                    "value": { "kind": "scalar", "value": true },
                }),
            ));
        }
        lines
            .into_iter()
            .map(|mut line| {
                line["drvPath"] = json!(drv_path);
                line.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn stage2_output(target_key: &str, drv_path: &str, identities: &[(&str, u64)]) -> String {
        stage2_output_with_digest(target_key, drv_path, TEST_DIGEST, identities)
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
        let result = reconcile_inspector_output(output.as_bytes(), &test_target()).unwrap();

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
    fn partial_provenance_uses_exact_observed_set_beyond_diagnostic_budget() {
        let path = ["healthy", "after"];
        let key = hash(&path);
        let mut provenance = supported_provenance(&path, &key);
        let healthy_group = provenance["definitionsByOption"][0].clone();
        let mut groups = vec![healthy_group];
        groups.extend((0..130).map(|index| {
            let omitted_path = vec!["omitted".to_string(), format!("branch{index:03}")];
            let omitted_key = option_key(&omitted_path);
            let mut group = provenance["definitionsByOption"][0].clone();
            group["option_key"] = json!(omitted_key);
            group["path"] = json!(omitted_path);
            group
        }));
        provenance["definitionsByOption"] = json!(groups);

        let mut partial_index = index(&key, &path);
        partial_index["optionInventoryComplete"] = json!(false);
        partial_index["optionInventoryDiagnostics"] = json!(
            (0..MAX_OPTION_INVENTORY_DIAGNOSTICS)
                .map(|index| unreadable_diagnostic(&format!("poison{index:03}")))
                .collect::<Vec<_>>()
        );
        partial_index["optionInventoryDiagnosticsTruncated"] = json!(true);
        let output = [
            result(INDEX_ATTRIBUTE, partial_index),
            result(&format!("meta_{key}"), metadata(&key, &path)),
            result(
                &format!("value_{key}"),
                json!({
                    "kind": "value",
                    "key": key,
                    "value": { "kind": "scalar", "value": "after" },
                }),
            ),
            result(PROVENANCE_ATTRIBUTE, provenance.clone()),
        ]
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
        let stage1 = reconcile_inspector_output(output.as_bytes(), &test_target())
            .expect("partial provenance should reconcile");
        let InspectionProvenance::Available {
            provenance_digest,
            definitions_by_option,
            ..
        } = &stage1.provenance
        else {
            panic!("partial provenance should remain available");
        };
        assert_ne!(provenance_digest, TEST_DIGEST);
        assert_eq!(definitions_by_option.len(), 1);
        assert_eq!(definitions_by_option[0].option_key, key);

        let stage2_output = stage2_output_with_digest(
            &stage1.target_key,
            &stage1.carrier_drv_path,
            provenance_digest,
            &[(&key, 0), (&key, 1)],
        );
        assert!(matches!(
            reconcile_definition_values_output(stage2_output.as_bytes(), &stage1),
            Ok(DefinitionValueEnrichment::Available { values, .. }) if values.len() == 2
        ));

        let complete = reconcile_inspector_output(
            output_with_provenance(&path, &key, provenance).as_bytes(),
            &test_target(),
        )
        .expect("complete core inspection should remain available");
        assert!(matches!(
            complete.provenance,
            InspectionProvenance::Unavailable { ref reason_code, .. }
                if reason_code == "provenance_integrity_failure"
        ));
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
        let result = reconcile_inspector_output(output.as_bytes(), &test_target()).unwrap();

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
    fn supported_provenance_requires_explicit_definition_list() {
        let path = ["healthy", "option"];
        let key = hash(&path);
        let mut missing = supported_provenance(&path, &key);
        missing
            .as_object_mut()
            .expect("provenance fixture is an object")
            .remove("definitionsByOption");
        let result = reconcile_inspector_output(
            output_with_provenance(&path, &key, missing).as_bytes(),
            &test_target(),
        )
        .expect("core inspection remains available");
        assert!(matches!(
            result.provenance,
            InspectionProvenance::Unavailable { ref reason_code, .. }
                if reason_code == "malformed_payload"
        ));

        let mut empty = supported_provenance(&path, &key);
        empty["definitionsByOption"] = json!([]);
        let result = reconcile_inspector_output(
            output_with_provenance(&path, &key, empty).as_bytes(),
            &test_target(),
        )
        .expect("an explicit empty definition list is valid");
        assert!(matches!(
            result.provenance,
            InspectionProvenance::Available {
                definitions_by_option,
                ..
            } if definitions_by_option.is_empty()
        ));
    }

    #[test]
    fn unsupported_provenance_preserves_helper_capability_reason() {
        let path = ["healthy", "option"];
        let key = hash(&path);
        let payload = json!({
            "adapterVersion": EXPECTED_PROVENANCE_ADAPTER_VERSION,
            "supported": false,
            "reasonCode": "helper_capability_unavailable",
        });
        let result = reconcile_inspector_output(
            output_with_provenance(&path, &key, payload).as_bytes(),
            &test_target(),
        )
        .expect("unsupported provenance does not hide core inspection");
        assert!(matches!(
            result.provenance,
            InspectionProvenance::Unavailable { ref reason_code, .. }
                if reason_code == "helper_capability_unavailable"
        ));
    }

    #[test]
    fn definition_ordinals_must_be_a_zero_based_permutation() {
        let path = ["crystalForgeProbe", "target"];
        let key = hash(&path);
        for ordinals in [[1, 2], [0, 2], [0, 0]] {
            let mut payload = supported_provenance(&path, &key);
            payload["definitionsByOption"][0]["definitions"][0]["ordinal"] = json!(ordinals[0]);
            payload["definitionsByOption"][0]["definitions"][1]["ordinal"] = json!(ordinals[1]);
            let result = reconcile_inspector_output(
                output_with_provenance(&path, &key, payload).as_bytes(),
                &test_target(),
            )
            .expect("core inspection remains available");
            assert!(matches!(
                result.provenance,
                InspectionProvenance::Unavailable { ref reason_code, .. }
                    if reason_code == "provenance_integrity_failure"
            ));
        }
    }

    #[test]
    fn surviving_merge_orders_must_be_a_zero_based_permutation() {
        let path = ["crystalForgeProbe", "target"];
        let key = hash(&path);
        for orders in [[1, 2], [0, 2], [0, 0]] {
            let mut payload = supported_provenance(&path, &key);
            payload["definitionsByOption"][0]["definitions"][0]["status"] =
                json!("active_surviving");
            payload["definitionsByOption"][0]["definitions"][0]["surviving_merge_order"] =
                json!(orders[0]);
            payload["definitionsByOption"][0]["definitions"][1]["surviving_merge_order"] =
                json!(orders[1]);
            let result = reconcile_inspector_output(
                output_with_provenance(&path, &key, payload).as_bytes(),
                &test_target(),
            )
            .expect("core inspection remains available");
            assert!(matches!(
                result.provenance,
                InspectionProvenance::Unavailable { ref reason_code, .. }
                    if reason_code == "provenance_integrity_failure"
            ));
        }

        let mut discarded = supported_provenance(&path, &key);
        discarded["definitionsByOption"][0]["definitions"][0]["surviving_merge_order"] = json!(1);
        let result = reconcile_inspector_output(
            output_with_provenance(&path, &key, discarded).as_bytes(),
            &test_target(),
        )
        .expect("core inspection remains available");
        assert!(matches!(
            result.provenance,
            InspectionProvenance::Unavailable { ref reason_code, .. }
                if reason_code == "provenance_integrity_failure"
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
        let result = reconcile_inspector_output(output.as_bytes(), &test_target()).unwrap();

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

        assert!(reconcile_inspector_output(output.as_bytes(), &test_target()).is_err());
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

    #[test]
    fn inspection_target_key_is_structured_and_configuration_scoped() {
        assert_ne!(
            inspection_target_key("path:/flake", "one"),
            inspection_target_key("path:/flake", "two")
        );
        assert_ne!(
            inspection_target_key("path:/flake-a", "one"),
            inspection_target_key("path:/flake-b", "one")
        );
        let values = [
            "dots.name",
            "slashes/name",
            "quotes\"name",
            "white space",
            "${builtins.abort \"no\"}",
        ];
        let keys = values
            .iter()
            .map(|value| inspection_target_key(value, value))
            .collect::<HashSet<_>>();
        assert_eq!(keys.len(), values.len());
        assert!(keys.iter().all(|key| is_digest(key)));
    }

    #[test]
    fn stage2_rejects_target_mismatch_before_correlating_values() {
        let expected = stage1_context();
        let key = hash(&["crystalForgeProbe", "target"]);
        let output = stage2_output(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "/nix/store/shared.drv",
            &[(&key, 0), (&key, 1)],
        );
        assert!(matches!(
            reconcile_definition_values_output(output.as_bytes(), &expected).unwrap(),
            DefinitionValueEnrichment::Unavailable { reason_code, .. }
                if reason_code == "inspection_target_mismatch"
        ));
    }

    #[test]
    fn stage2_rejects_carrier_mismatch_even_when_stage2_is_self_consistent() {
        let expected = stage1_context();
        let key = hash(&["crystalForgeProbe", "target"]);
        let output = stage2_output(
            TEST_TARGET_KEY,
            "/nix/store/different.drv",
            &[(&key, 0), (&key, 1)],
        );
        assert!(matches!(
            reconcile_definition_values_output(output.as_bytes(), &expected).unwrap(),
            DefinitionValueEnrichment::Unavailable { reason_code, .. }
                if reason_code == "carrier_derivation_mismatch"
        ));
    }

    #[test]
    fn stage2_rejects_flake_source_mismatch() {
        let expected = stage1_context();
        let key = hash(&["crystalForgeProbe", "target"]);
        let output = stage2_output(
            TEST_TARGET_KEY,
            "/nix/store/shared.drv",
            &[(&key, 0), (&key, 1)],
        )
        .replace(TEST_SOURCE_OUT_PATH, "/nix/store/different-flake-source");
        assert!(matches!(
            reconcile_definition_values_output(output.as_bytes(), &expected).unwrap(),
            DefinitionValueEnrichment::Unavailable { reason_code, .. }
                if reason_code == "flake_source_mismatch"
        ));
    }

    #[test]
    fn stage2_index_failure_is_explicit_and_redacted() {
        let expected = stage1_context();
        let mut line = result("__crystalForgeDefinitionIndex", json!(null));
        line["error"] = json!("https://user:secret@example.com/repo?token=query-secret");
        assert!(matches!(
            reconcile_definition_values_output(line.to_string().as_bytes(), &expected).unwrap(),
            DefinitionValueEnrichment::Unavailable {
                reason_code,
                diagnostic: Some(SafeEvaluationError { message, .. }),
            } if reason_code == "stage2_index_failed"
                && !message.contains("secret")
                && !message.contains("query-secret")
        ));
    }

    #[test]
    fn stage2_accepts_exact_target_carrier_digest_and_identities() {
        let expected = stage1_context();
        let key = hash(&["crystalForgeProbe", "target"]);
        let output = stage2_output(
            TEST_TARGET_KEY,
            "/nix/store/shared.drv",
            &[(&key, 0), (&key, 1)],
        );
        assert!(matches!(
            reconcile_definition_values_output(output.as_bytes(), &expected).unwrap(),
            DefinitionValueEnrichment::Available { values, .. } if values.len() == 2
        ));
    }

    #[test]
    fn duplicate_stage2_index_identities_are_integrity_errors() {
        let expected = stage1_context();
        let key = hash(&["crystalForgeProbe", "target"]);
        let mut lines = stage2_output(
            TEST_TARGET_KEY,
            "/nix/store/shared.drv",
            &[(&key, 0), (&key, 1)],
        )
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        let mut index = serde_json::from_str::<Value>(&lines[0]).expect("Stage-2 index is JSON");
        index["extraValue"]["definitions"][1]["ordinal"] = json!(0);
        lines[0] = index.to_string();
        assert!(
            reconcile_definition_values_output(lines.join("\n").as_bytes(), &expected).is_err()
        );
    }

    #[test]
    fn stage2_supported_field_is_required() {
        let expected = stage1_context();
        let key = hash(&["crystalForgeProbe", "target"]);
        let mut line = serde_json::from_str::<Value>(
            &stage2_output(
                TEST_TARGET_KEY,
                "/nix/store/shared.drv",
                &[(&key, 0), (&key, 1)],
            )
            .lines()
            .next()
            .expect("Stage-2 index exists"),
        )
        .expect("Stage-2 index is JSON");
        line["extraValue"]
            .as_object_mut()
            .unwrap()
            .remove("supported");
        let output = line.to_string();
        assert!(reconcile_definition_values_output(output.as_bytes(), &expected).is_err());
    }

    #[test]
    fn unsupported_reason_codes_are_sanitized() {
        let path = ["healthy", "option"];
        let key = hash(&path);
        let stage1 = stage1_context();
        for reason in [
            Some("helper_capability_unavailable"),
            Some("not-evaluator-text"),
            None,
        ] {
            let mut payload = json!({
                "adapterVersion": EXPECTED_PROVENANCE_ADAPTER_VERSION,
                "supported": false,
            });
            if let Some(reason) = reason {
                payload["reasonCode"] = json!(reason);
            }
            let stage1_result = reconcile_inspector_output(
                output_with_provenance(&path, &key, payload).as_bytes(),
                &test_target(),
            )
            .expect("unsupported provenance preserves Stage 1");
            let expected = if reason == Some("helper_capability_unavailable") {
                "helper_capability_unavailable"
            } else {
                "adapter_unsupported"
            };
            assert!(matches!(
                stage1_result.provenance,
                InspectionProvenance::Unavailable { ref reason_code, .. }
                    if reason_code == expected
            ));

            let mut index = json!({
                "kind": "definition_index",
                "targetKey": TEST_TARGET_KEY,
                "sourceOutPath": TEST_SOURCE_OUT_PATH,
                "adapterVersion": EXPECTED_PROVENANCE_ADAPTER_VERSION,
                "supported": false,
                "definitionCount": 0,
                "definitions": [],
            });
            if let Some(reason) = reason {
                index["reasonCode"] = json!(reason);
            }
            let mut line = result("__crystalForgeDefinitionIndex", index);
            line["drvPath"] = json!("/nix/store/shared.drv");
            let enrichment =
                reconcile_definition_values_output(line.to_string().as_bytes(), &stage1)
                    .expect("unsupported Stage-2 index preserves Stage 1");
            assert!(matches!(
                enrichment,
                DefinitionValueEnrichment::Unavailable { reason_code, .. }
                    if reason_code == expected
            ));
        }
    }

    #[test]
    fn assembly_retains_discarded_definitions_and_joins_values_by_identity() {
        let path = ["services", "demo", "enable"];
        let key = hash(&path);
        let stage1 = semantic_stage1(
            vec![semantic_option(
                &key,
                &path,
                InspectionMetadata::Available(semantic_metadata(&path)),
                semantic_value(true),
            )],
            semantic_provenance(vec![RawDefinitionsForOption {
                option_key: key.clone(),
                path: path.iter().map(|part| (*part).to_string()).collect(),
                definitions: vec![
                    semantic_definition(1, 50, RawDefinitionStatus::PriorityDiscarded, None, None),
                    semantic_definition(
                        0,
                        100,
                        RawDefinitionStatus::ActiveSurviving,
                        Some("/nix/store/winner.nix"),
                        Some(0),
                    ),
                ],
            }]),
        );
        let assembled = assemble_config_inspection(
            stage1,
            semantic_stage2(vec![
                semantic_definition_value(&key, 1, semantic_value(false)),
                semantic_definition_value(&key, 0, semantic_value(true)),
            ]),
        )
        .unwrap();

        let AssembledOptionProvenance::Available {
            definitions,
            override_state,
        } = &assembled.options[0].provenance
        else {
            panic!("expected available provenance");
        };
        assert_eq!(definitions.len(), 2);
        assert_eq!(
            assembled.options[0].path_components,
            path.iter()
                .map(|part| (*part).to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(definitions[0].ordinal, 0);
        assert_eq!(definitions[1].ordinal, 1);
        assert_eq!(definitions[1].source_path, None);
        assert!(matches!(override_state, OverrideState::Known(true)));
        assert!(matches!(
            assembled.provenance_state,
            AssembledConfigProvenanceState::Available {
                definition_value_enrichment: DefinitionValueEnrichmentState::Available { .. },
                ..
            }
        ));
        assert!(matches!(
            definitions[0].value,
            Some(InspectionValue::Available(_))
        ));
        assert!(matches!(
            definitions[1].value,
            Some(InspectionValue::Available(_))
        ));
    }

    #[test]
    fn assembly_preserves_multiple_survivors_and_known_no_override() {
        let path = ["system", "feature"];
        let key = hash(&path);
        let stage1 = semantic_stage1(
            vec![semantic_option(
                &key,
                &path,
                InspectionMetadata::Available(semantic_metadata(&path)),
                semantic_value(true),
            )],
            semantic_provenance(vec![RawDefinitionsForOption {
                option_key: key.clone(),
                path: path.iter().map(|part| (*part).to_string()).collect(),
                definitions: vec![
                    semantic_definition(
                        1,
                        100,
                        RawDefinitionStatus::ActiveSurviving,
                        Some("/nix/store/two.nix"),
                        Some(1),
                    ),
                    semantic_definition(
                        0,
                        100,
                        RawDefinitionStatus::ActiveSurviving,
                        Some("/nix/store/zero.nix"),
                        Some(0),
                    ),
                ],
            }]),
        );
        let assembled = assemble_config_inspection(
            stage1,
            semantic_stage2(vec![
                semantic_definition_value(&key, 1, semantic_value(false)),
                semantic_definition_value(&key, 0, semantic_value(true)),
            ]),
        )
        .unwrap();
        let AssembledOptionProvenance::Available {
            definitions,
            override_state,
            ..
        } = &assembled.options[0].provenance
        else {
            panic!("expected available provenance");
        };
        assert_eq!(
            definitions
                .iter()
                .map(|definition| definition.ordinal)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(definitions[0].surviving_merge_order, Some(0));
        assert_eq!(definitions[1].surviving_merge_order, Some(1));
        assert!(matches!(override_state, OverrideState::Known(false)));
    }

    fn definition_group_for_test(
        definitions: Vec<RawDefinition>,
    ) -> (RawDefinitionsForOption, DefinitionValueEnrichmentState) {
        (
            RawDefinitionsForOption {
                option_key: "test-option".to_string(),
                path: vec!["test".to_string(), "option".to_string()],
                definitions,
            },
            DefinitionValueEnrichmentState::Unavailable {
                reason_code: "stage2_index_failed".to_string(),
                diagnostic: None,
            },
        )
    }

    #[test]
    fn assembly_requires_contiguous_zero_based_definition_ordinals() {
        for ordinals in [[0, 1], [0, 2], [1, 2], [0, 0]] {
            let definitions = ordinals
                .into_iter()
                .map(|ordinal| {
                    semantic_definition(
                        ordinal,
                        100,
                        RawDefinitionStatus::ActiveSurviving,
                        None,
                        Some(ordinal),
                    )
                })
                .collect();
            let (group, enrichment) = definition_group_for_test(definitions);
            let result = assemble_definitions("test-option", &group, &enrichment, &BTreeMap::new());
            if ordinals == [0, 1] {
                assert!(result.is_ok());
            } else {
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn assembly_requires_valid_surviving_merge_orders() {
        let valid = [Some(0), Some(1)];
        let invalid = [
            [Some(0), Some(2)],
            [Some(1), Some(2)],
            [Some(0), Some(0)],
            [None, Some(0)],
        ];

        let valid_definitions = valid
            .into_iter()
            .enumerate()
            .map(|(ordinal, merge_order)| {
                semantic_definition(
                    ordinal as u64,
                    100,
                    RawDefinitionStatus::ActiveSurviving,
                    None,
                    merge_order,
                )
            })
            .collect();
        let (group, enrichment) = definition_group_for_test(valid_definitions);
        assert!(
            assemble_definitions("test-option", &group, &enrichment, &BTreeMap::new(),).is_ok()
        );

        for merge_orders in invalid {
            let definitions = merge_orders
                .into_iter()
                .enumerate()
                .map(|(ordinal, merge_order)| {
                    semantic_definition(
                        ordinal as u64,
                        100,
                        RawDefinitionStatus::ActiveSurviving,
                        None,
                        merge_order,
                    )
                })
                .collect();
            let (group, enrichment) = definition_group_for_test(definitions);
            assert!(
                assemble_definitions("test-option", &group, &enrichment, &BTreeMap::new(),)
                    .is_err()
            );
        }

        let (group, enrichment) = definition_group_for_test(vec![semantic_definition(
            0,
            100,
            RawDefinitionStatus::PriorityDiscarded,
            None,
            Some(0),
        )]);
        assert!(
            assemble_definitions("test-option", &group, &enrichment, &BTreeMap::new(),).is_err()
        );
    }

    #[test]
    fn assembly_rejects_stage1_available_with_provenance_unavailable_reason() {
        let path = ["feature"];
        let key = hash(&path);
        let stage1 = semantic_stage1(
            vec![semantic_option(
                &key,
                &path,
                InspectionMetadata::Available(semantic_metadata(&path)),
                semantic_value(true),
            )],
            semantic_provenance(Vec::new()),
        );
        assert!(matches!(
            assemble_config_inspection(
                stage1,
                DefinitionValueEnrichment::Unavailable {
                    reason_code: "stage1_provenance_unavailable".to_string(),
                    diagnostic: None,
                },
            ),
            Err(_)
        ));
    }

    #[test]
    fn assembly_treats_omitted_definition_group_as_known_zero_definitions() {
        let path = ["feature"];
        let key = hash(&path);
        let assembled = assemble_config_inspection(
            semantic_stage1(
                vec![semantic_option(
                    &key,
                    &path,
                    InspectionMetadata::Available(semantic_metadata(&path)),
                    semantic_value(true),
                )],
                semantic_provenance(Vec::new()),
            ),
            semantic_stage2(Vec::new()),
        )
        .unwrap();

        assert!(matches!(
            assembled.options[0].provenance,
            AssembledOptionProvenance::Available {
                ref definitions,
                override_state: OverrideState::Known(false),
            } if definitions.is_empty()
        ));
        assert!(matches!(
            assembled.provenance_state,
            AssembledConfigProvenanceState::Available {
                definition_value_enrichment: DefinitionValueEnrichmentState::Available { .. },
                ..
            }
        ));
    }

    #[test]
    fn assembly_preserves_individual_failure_and_global_unavailability() {
        let path = ["feature"];
        let key = hash(&path);
        let failed = InspectionValue::Failed(SafeEvaluationError {
            code: "not_evaluated".to_string(),
            message: "safe failure".to_string(),
        });
        let stage1 = semantic_stage1(
            vec![semantic_option(
                &key,
                &path,
                InspectionMetadata::Available(semantic_metadata(&path)),
                semantic_value(true),
            )],
            semantic_provenance(vec![RawDefinitionsForOption {
                option_key: key.clone(),
                path: vec!["feature".to_string()],
                definitions: vec![semantic_definition(
                    0,
                    100,
                    RawDefinitionStatus::ActiveSurviving,
                    Some("/nix/store/feature.nix"),
                    Some(0),
                )],
            }]),
        );
        let assembled = assemble_config_inspection(
            stage1.clone(),
            semantic_stage2(vec![semantic_definition_value(&key, 0, failed.clone())]),
        )
        .unwrap();
        let AssembledOptionProvenance::Available { definitions, .. } =
            &assembled.options[0].provenance
        else {
            panic!("expected available provenance");
        };
        assert!(matches!(
            definitions[0].value,
            Some(InspectionValue::Failed(_))
        ));
        assert!(matches!(
            assembled.provenance_state,
            AssembledConfigProvenanceState::Available {
                definition_value_enrichment: DefinitionValueEnrichmentState::Available { .. },
                ..
            }
        ));

        let unavailable = assemble_config_inspection(
            stage1,
            DefinitionValueEnrichment::Unavailable {
                reason_code: "stage2_index_failed".to_string(),
                diagnostic: None,
            },
        )
        .unwrap();
        let AssembledOptionProvenance::Available {
            definitions,
            override_state,
        } = &unavailable.options[0].provenance
        else {
            panic!("expected available provenance");
        };
        assert_eq!(definitions[0].value, None);
        assert!(matches!(override_state, OverrideState::Known(false)));
        assert!(matches!(
            unavailable.provenance_state,
            AssembledConfigProvenanceState::Available {
                definition_value_enrichment: DefinitionValueEnrichmentState::Unavailable { reason_code, .. },
                ..
            }
                if reason_code == "stage2_index_failed"
        ));
    }

    #[test]
    fn assembly_preserves_provenance_unavailability_and_rejects_enrichment() {
        let path = ["feature"];
        let key = hash(&path);
        let stage1 = semantic_stage1(
            vec![semantic_option(
                &key,
                &path,
                InspectionMetadata::Available(semantic_metadata(&path)),
                semantic_value(true),
            )],
            InspectionProvenance::Unavailable {
                reason_code: "capability_self_test_failed".to_string(),
                diagnostic: None,
            },
        );
        let valid = assemble_config_inspection(
            stage1.clone(),
            DefinitionValueEnrichment::Unavailable {
                reason_code: "stage1_provenance_unavailable".to_string(),
                diagnostic: None,
            },
        )
        .unwrap();
        assert!(matches!(
            valid.options[0].provenance,
            AssembledOptionProvenance::Unavailable
        ));
        assert!(matches!(
            valid.provenance_state,
            AssembledConfigProvenanceState::Unavailable { ref reason_code, .. }
                if reason_code == "capability_self_test_failed"
        ));
        assert!(matches!(
            assemble_config_inspection(stage1, semantic_stage2(Vec::new())),
            Err(_)
        ));
    }

    #[test]
    fn assembly_preserves_metadata_and_effective_value_failures() {
        let path = ["feature"];
        let key = hash(&path);
        let metadata_failure = InspectionMetadata::Failed(SafeEvaluationError {
            code: "metadata_failed".to_string(),
            message: "metadata unavailable".to_string(),
        });
        let value_failure = InspectionValue::Failed(SafeEvaluationError {
            code: "not_evaluated".to_string(),
            message: "value unavailable".to_string(),
        });
        let stage1 = semantic_stage1(
            vec![semantic_option(
                &key,
                &path,
                metadata_failure.clone(),
                value_failure.clone(),
            )],
            semantic_provenance(Vec::new()),
        );
        let assembled = assemble_config_inspection(stage1, semantic_stage2(Vec::new())).unwrap();
        assert_eq!(assembled.options[0].metadata, metadata_failure);
        assert_eq!(assembled.options[0].effective_value, value_failure);
        assert!(matches!(
            assembled.options[0].provenance,
            AssembledOptionProvenance::Available {
                override_state: OverrideState::Known(false),
                ..
            }
        ));
    }

    #[test]
    fn assembly_rejects_identity_and_path_disagreements() {
        let path = ["foo", "bar", "baz"];
        let dotted_path = ["foo", "bar.baz"];
        let key = hash(&path);
        let unknown_stage1 = semantic_stage1(
            vec![semantic_option(
                &key,
                &path,
                InspectionMetadata::Available(semantic_metadata(&path)),
                semantic_value(true),
            )],
            semantic_provenance(vec![RawDefinitionsForOption {
                option_key: "unknown".to_string(),
                path: vec!["unknown".to_string()],
                definitions: Vec::new(),
            }]),
        );
        assert!(matches!(
            assemble_config_inspection(unknown_stage1, semantic_stage2(Vec::new())),
            Err(_)
        ));

        let stage1 = semantic_stage1(
            vec![semantic_option(
                &key,
                &path,
                InspectionMetadata::Available(semantic_metadata(&path)),
                semantic_value(true),
            )],
            semantic_provenance(vec![RawDefinitionsForOption {
                option_key: key.clone(),
                path: dotted_path.iter().map(|part| (*part).to_string()).collect(),
                definitions: Vec::new(),
            }]),
        );
        assert!(matches!(
            assemble_config_inspection(stage1, semantic_stage2(Vec::new())),
            Err(_)
        ));

        let path = ["feature"];
        let key = hash(&path);
        let stage1 = semantic_stage1(
            vec![semantic_option(
                &key,
                &path,
                InspectionMetadata::Available(semantic_metadata(&path)),
                semantic_value(true),
            )],
            semantic_provenance(vec![RawDefinitionsForOption {
                option_key: key.clone(),
                path: vec!["feature".to_string()],
                definitions: vec![semantic_definition(
                    0,
                    100,
                    RawDefinitionStatus::ActiveSurviving,
                    None,
                    Some(0),
                )],
            }]),
        );
        assert!(matches!(
            assemble_config_inspection(
                stage1,
                semantic_stage2(vec![semantic_definition_value(
                    &key,
                    1,
                    semantic_value(true)
                )])
            ),
            Err(_)
        ));
    }

    #[test]
    fn assembly_rejects_duplicate_and_mismatched_stage2_metadata() {
        let path = ["feature"];
        let key = hash(&path);
        let stage1 = semantic_stage1(
            vec![semantic_option(
                &key,
                &path,
                InspectionMetadata::Available(semantic_metadata(&path)),
                semantic_value(true),
            )],
            semantic_provenance(vec![RawDefinitionsForOption {
                option_key: key.clone(),
                path: vec!["feature".to_string()],
                definitions: vec![semantic_definition(
                    0,
                    100,
                    RawDefinitionStatus::ActiveSurviving,
                    None,
                    Some(0),
                )],
            }]),
        );
        assert!(matches!(
            assemble_config_inspection(
                stage1.clone(),
                semantic_stage2(vec![
                    semantic_definition_value(&key, 0, semantic_value(true)),
                    semantic_definition_value(&key, 0, semantic_value(false)),
                ])
            ),
            Err(_)
        ));
        assert!(matches!(
            assemble_config_inspection(
                stage1,
                DefinitionValueEnrichment::Available {
                    adapter_version: EXPECTED_PROVENANCE_ADAPTER_VERSION + 1,
                    provenance_digest: TEST_DIGEST.to_string(),
                    values: vec![semantic_definition_value(&key, 0, semantic_value(true))],
                }
            ),
            Err(_)
        ));
    }
}
