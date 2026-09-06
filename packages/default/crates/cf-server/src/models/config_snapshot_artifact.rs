//! Defines the truthful, server-internal V2 config snapshot artifact.
//!
//! This module converts the validated semantic inspector result into the
//! versioned contract that a later persistence layer can store. It does not
//! perform evaluation or database work. Callers must redact the artifact
//! before calculating a digest, building search text, or persisting it.

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::config_inspector::option_key as canonical_option_key;
use super::config_inspector::{
    AssembledConfigInspection, AssembledConfigProvenanceState, AssembledDefinition,
    AssembledOption, AssembledOptionProvenance, DefinitionValueEnrichmentState, InspectionMetadata,
    InspectionValue, OverrideState, RawDefinitionStatus,
};
use super::evaluation_snapshots::{SafeEvaluationError, SafeOptionValue};
use crate::security::snapshot_redaction::{
    REDACTED_VALUE, redact_evaluation_error, redact_json, redact_option_value, redact_text,
};

/// Identifies the internal config artifact contract implemented by this module.
pub(crate) const CONFIG_OPTION_ARTIFACT_SCHEMA_VERSION_V2: u32 = 2;
const MAX_SEARCH_TEXT_CHARS: usize = 16_384;

/// Contains one complete, versioned config inspection artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ConfigInspectionArtifactV2 {
    /// Version of the internal artifact contract.
    pub artifact_version: u32,
    /// Immutable inspection target identity.
    pub target_key: String,
    /// Resolved immutable flake source path.
    pub source_out_path: String,
    /// Shared evaluation carrier derivation path.
    pub carrier_drv_path: String,
    /// Configuration-global provenance and enrichment state.
    pub provenance_state: ConfigProvenanceArtifactStateV2,
    /// Options in validated Stage-1 index order.
    pub options: Vec<ConfigOptionArtifactV2>,
}

/// Preserves configuration-global provenance and definition-value state once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum ConfigProvenanceArtifactStateV2 {
    /// Provenance metadata and global enrichment are available.
    Available {
        /// Provenance adapter version.
        adapter_version: u64,
        /// Target library version, when available.
        target_lib_version: Option<String>,
        /// Target module-system source path, when available.
        target_module_system_path: Option<String>,
        /// Canonical raw-definition digest.
        provenance_digest: String,
        /// Global Stage-2 definition-value state.
        definition_value_enrichment: DefinitionValueArtifactStateV2,
    },
    /// Configuration-wide raw provenance was not established.
    Unavailable {
        /// Stable unavailable reason.
        reason_code: String,
        /// Sanitized diagnostic, when available.
        diagnostic: Option<SafeEvaluationError>,
    },
}

/// Contains one option without collapsing failed or unavailable states.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ConfigOptionArtifactV2 {
    /// Collision-resistant option identity.
    pub option_key: String,
    /// Exact authoritative path components.
    pub path_components: Vec<String>,
    /// Explicit metadata result.
    pub metadata: ConfigOptionMetadataArtifactV2,
    /// Effective option value or explicit evaluation failure.
    pub effective_value: SafeOptionValue,
    /// Explicit raw-provenance result.
    pub provenance: ConfigOptionProvenanceArtifactV2,
}

/// Preserves metadata success and failure as separate artifact states.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum ConfigOptionMetadataArtifactV2 {
    /// Metadata was evaluated successfully.
    Available {
        /// Internal Nix option marker.
        option_type: Option<String>,
        /// Source location components.
        loc: Vec<String>,
        /// Nix declared type.
        declared_type: Option<String>,
        /// Declaration source paths.
        declarations: Vec<String>,
        /// Safe declaration position metadata.
        declaration_positions: Vec<Value>,
        /// Highest surviving module priority.
        highest_prio: Option<i64>,
        /// Whether the selected option has a surviving definition.
        is_defined: bool,
        /// Basic surviving-source metadata.
        surviving_definition_sources: Vec<ConfigDefinitionSourceArtifactV2>,
    },
    /// Metadata evaluation failed.
    Failed {
        /// Sanitized evaluation failure.
        error: SafeEvaluationError,
    },
}

/// Preserves basic metadata about a surviving definition source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ConfigDefinitionSourceArtifactV2 {
    /// Source path reported by the module system.
    pub source_path: String,
    /// Module priority, when available.
    pub priority: Option<i64>,
    /// Resolved flake input, when available.
    pub source_input: Option<String>,
    /// Resolved source revision, when available.
    pub source_revision: Option<String>,
}

/// Preserves whether raw provenance is available.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum ConfigOptionProvenanceArtifactV2 {
    /// Raw definitions and local override state are available.
    Available {
        /// All raw definitions in ordinal order.
        definitions: Vec<ConfigDefinitionArtifactV2>,
        /// Proven override state.
        override_state: bool,
    },
    /// Raw provenance was not established.
    Unavailable,
}

/// Contains one raw definition and its optional safe value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ConfigDefinitionArtifactV2 {
    /// Owning option identity.
    pub option_key: String,
    /// Definition identity within the option.
    pub ordinal: u64,
    /// Source path, when supplied by the module system.
    pub source_path: Option<String>,
    /// Flake input, when resolved.
    pub source_input: Option<String>,
    /// Source revision, when resolved.
    pub source_revision: Option<String>,
    /// Module-system key, when supplied.
    pub module_key: Option<String>,
    /// Module-system priority.
    pub priority: i64,
    /// Precise raw definition status.
    pub status: ConfigDefinitionStatusV2,
    /// Merge order among active survivors, when applicable.
    pub surviving_merge_order: Option<u64>,
    /// Definition value, absent only when global Stage 2 was unavailable.
    pub value: Option<SafeOptionValue>,
}

/// Identifies the raw status of a definition without collapsing survivors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConfigDefinitionStatusV2 {
    /// Definition participates at the winning priority.
    ActiveSurviving,
    /// Definition was discarded by priority filtering.
    PriorityDiscarded,
}

/// Preserves the global state of definition-value enrichment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum DefinitionValueArtifactStateV2 {
    /// Every definition has one available or failed value.
    Available {
        /// Adapter version bound to the values.
        adapter_version: u64,
        /// Provenance digest bound to the values.
        provenance_digest: String,
    },
    /// No per-definition value was established.
    Unavailable {
        /// Stable unavailable reason.
        reason_code: String,
        /// Sanitized diagnostic, when available.
        diagnostic: Option<SafeEvaluationError>,
    },
}

/// Converts validated semantic inspection into the internal V2 artifact.
pub(crate) fn config_artifact_v2_from_assembled(
    assembled: AssembledConfigInspection,
) -> Result<ConfigInspectionArtifactV2> {
    if !is_key(&assembled.target_key) {
        bail!("invalid config artifact target key");
    }
    if assembled.source_out_path.is_empty() {
        bail!("config artifact source path is empty");
    }
    if assembled.carrier_drv_path.is_empty() {
        bail!("config artifact carrier path is empty");
    }
    let provenance_state = config_provenance_artifact(&assembled.provenance_state)?;

    let mut option_keys = BTreeSet::new();
    let options = assembled
        .options
        .into_iter()
        .map(|option| {
            if !option_keys.insert(option.option_key.clone()) {
                bail!("duplicate config artifact option key");
            }
            config_option_artifact(option, &provenance_state)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ConfigInspectionArtifactV2 {
        artifact_version: CONFIG_OPTION_ARTIFACT_SCHEMA_VERSION_V2,
        target_key: assembled.target_key,
        source_out_path: assembled.source_out_path,
        carrier_drv_path: assembled.carrier_drv_path,
        provenance_state,
        options,
    })
}

impl ConfigInspectionArtifactV2 {
    /// Returns a redacted artifact safe for digesting, indexing, or persistence.
    pub(crate) fn redacted(mut self) -> Self {
        self.provenance_state = redact_global_provenance(self.provenance_state);
        self.options = self.options.into_iter().map(redact_option).collect();
        self
    }

    /// Returns the SHA-256 digest of namespaced, redacted option-local content.
    ///
    /// The digest contains the V2 artifact schema version as a content-addressing
    /// namespace. It excludes option paths, target identity, and configuration-
    /// global provenance metadata, so equal local semantics deduplicate within
    /// V2 while remaining distinct from content in another artifact schema.
    /// Global inspection completeness belongs to the parent artifact and must be
    /// considered separately by future comparison queries.
    pub(crate) fn option_content_digest(option: &ConfigOptionArtifactV2) -> [u8; 32] {
        let redacted = redact_option(option.clone());
        let content = option_content_projection(&redacted);
        let namespaced_content = json!({
            "schema_version": CONFIG_OPTION_ARTIFACT_SCHEMA_VERSION_V2,
            "content": content,
        });
        let bytes = serde_json::to_vec(&namespaced_content).unwrap_or_default();
        Sha256::digest(bytes).into()
    }

    /// Returns bounded searchable text for redacted non-path option content.
    pub(crate) fn option_search_text(option: &ConfigOptionArtifactV2) -> String {
        let redacted = redact_option(option.clone());
        let content = option_content_projection(&redacted);
        serde_json::to_string(&content)
            .unwrap_or_default()
            .replace(REDACTED_VALUE, "")
            .chars()
            .take(MAX_SEARCH_TEXT_CHARS)
            .collect()
    }

    /// Returns the canonical redacted JSON payload for one option's local content.
    pub(crate) fn option_content_payload(option: &ConfigOptionArtifactV2) -> Value {
        option_content_projection(&redact_option(option.clone()))
    }

    /// Returns the redacted configuration-global provenance JSON value.
    pub(crate) fn provenance_state_payload(&self) -> Result<Value, serde_json::Error> {
        serde_json::to_value(&self.provenance_state)
    }

    /// Reports whether this artifact supports a complete semantic comparison.
    pub(crate) fn comparison_ready(&self) -> bool {
        matches!(
            self.provenance_state,
            ConfigProvenanceArtifactStateV2::Available {
                definition_value_enrichment: DefinitionValueArtifactStateV2::Available { .. },
                ..
            }
        )
    }
}

/// Reconstructs one V2 option from its authoritative identity and persisted local payload.
///
/// The database stores `option_key` and `path_components` beside a local payload
/// that intentionally omits both fields. This helper restores the identity for
/// semantic readers and rejects payloads that do not match the exact V2 shape.
pub(crate) fn config_option_v2_from_persisted(
    option_key: String,
    path_components: Vec<String>,
    mut payload: Value,
) -> Result<ConfigOptionArtifactV2> {
    if path_components.is_empty() || path_components.iter().any(String::is_empty) {
        bail!("persisted V2 option path is empty");
    }
    if canonical_option_key(&path_components) != option_key {
        bail!("persisted V2 option key does not match path components");
    }

    let object = payload
        .as_object_mut()
        .context("persisted V2 option payload is not an object")?;
    let expected = ["metadata", "effective_value", "provenance"];
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        bail!("persisted V2 option payload has an invalid local shape");
    }

    let provenance = object
        .get_mut("provenance")
        .and_then(Value::as_object_mut)
        .context("persisted V2 option provenance is not an object")?;
    if provenance.get("state").and_then(Value::as_str) == Some("available") {
        let definitions = provenance
            .get_mut("definitions")
            .and_then(Value::as_array_mut)
            .context("persisted V2 definitions are not an array")?;
        for definition in definitions {
            let definition = definition
                .as_object_mut()
                .context("persisted V2 definition is not an object")?;
            if definition.contains_key("option_key") {
                bail!("persisted V2 definition contains an unexpected option key");
            }
            definition.insert("option_key".to_string(), Value::String(option_key.clone()));
        }
    }

    let mut complete = object.clone();
    complete.insert("option_key".to_string(), Value::String(option_key.clone()));
    complete.insert(
        "path_components".to_string(),
        Value::Array(path_components.iter().cloned().map(Value::String).collect()),
    );
    let option: ConfigOptionArtifactV2 = serde_json::from_value(Value::Object(complete))
        .context("persisted V2 option payload failed semantic decoding")?;
    if option.option_key != option_key || option.path_components != path_components {
        bail!("persisted V2 option identity changed during decoding");
    }
    if let ConfigOptionProvenanceArtifactV2::Available { definitions, .. } = &option.provenance {
        if definitions
            .iter()
            .any(|definition| definition.option_key != option_key)
        {
            bail!("persisted V2 definition owner does not match option identity");
        }
    }
    Ok(option)
}

fn redact_option(mut option: ConfigOptionArtifactV2) -> ConfigOptionArtifactV2 {
    let context = redaction_context(&option.path_components);
    option.metadata = redact_metadata(std::mem::replace(
        &mut option.metadata,
        ConfigOptionMetadataArtifactV2::Failed {
            error: SafeEvaluationError {
                code: "internal_redaction_placeholder".to_string(),
                message: REDACTED_VALUE.to_string(),
            },
        },
    ));
    option.effective_value = redact_safe_value(&context, option.effective_value);
    option.provenance = redact_provenance(
        std::mem::replace(
            &mut option.provenance,
            ConfigOptionProvenanceArtifactV2::Unavailable,
        ),
        &context,
    );
    option
}

fn redact_global_provenance(
    state: ConfigProvenanceArtifactStateV2,
) -> ConfigProvenanceArtifactStateV2 {
    match state {
        ConfigProvenanceArtifactStateV2::Available {
            adapter_version,
            target_lib_version,
            target_module_system_path,
            provenance_digest,
            definition_value_enrichment,
        } => ConfigProvenanceArtifactStateV2::Available {
            adapter_version,
            target_lib_version: target_lib_version.map(|value| redact_text(&value)),
            target_module_system_path: target_module_system_path.map(|value| redact_text(&value)),
            provenance_digest: redact_text(&provenance_digest),
            definition_value_enrichment: redact_enrichment(definition_value_enrichment),
        },
        ConfigProvenanceArtifactStateV2::Unavailable {
            reason_code,
            diagnostic,
        } => ConfigProvenanceArtifactStateV2::Unavailable {
            reason_code: redact_text(&reason_code),
            diagnostic: diagnostic.map(redact_error),
        },
    }
}

fn config_provenance_artifact(
    state: &AssembledConfigProvenanceState,
) -> Result<ConfigProvenanceArtifactStateV2> {
    Ok(match state {
        AssembledConfigProvenanceState::Available {
            adapter_version,
            target_lib_version,
            target_module_system_path,
            provenance_digest,
            definition_value_enrichment,
        } => ConfigProvenanceArtifactStateV2::Available {
            adapter_version: *adapter_version,
            target_lib_version: target_lib_version.clone(),
            target_module_system_path: target_module_system_path.clone(),
            provenance_digest: provenance_digest.clone(),
            definition_value_enrichment: enrichment_artifact(definition_value_enrichment)?,
        },
        AssembledConfigProvenanceState::Unavailable {
            reason_code,
            diagnostic,
        } => ConfigProvenanceArtifactStateV2::Unavailable {
            reason_code: reason_code.clone(),
            diagnostic: diagnostic.clone(),
        },
    })
}

fn config_option_artifact(
    option: AssembledOption,
    global_provenance: &ConfigProvenanceArtifactStateV2,
) -> Result<ConfigOptionArtifactV2> {
    if !is_key(&option.option_key) {
        bail!("invalid config artifact option key");
    }
    if option.path_components.is_empty() || option.path_components.iter().any(String::is_empty) {
        bail!("config artifact option path is empty");
    }
    let metadata = metadata_artifact(&option.path_components, option.metadata)?;
    let effective_value = inspection_value(option.effective_value);
    let provenance = provenance_artifact(&option.option_key, &option.provenance)?;
    validate_option_provenance(global_provenance, &provenance)?;
    Ok(ConfigOptionArtifactV2 {
        option_key: option.option_key,
        path_components: option.path_components,
        metadata,
        effective_value,
        provenance,
    })
}

fn validate_option_provenance(
    global: &ConfigProvenanceArtifactStateV2,
    local: &ConfigOptionProvenanceArtifactV2,
) -> Result<()> {
    match (global, local) {
        (
            ConfigProvenanceArtifactStateV2::Available {
                definition_value_enrichment,
                ..
            },
            ConfigOptionProvenanceArtifactV2::Available { definitions, .. },
        ) => validate_definition_values(definitions, definition_value_enrichment),
        (
            ConfigProvenanceArtifactStateV2::Unavailable { .. },
            ConfigOptionProvenanceArtifactV2::Unavailable,
        ) => Ok(()),
        (
            ConfigProvenanceArtifactStateV2::Available { .. },
            ConfigOptionProvenanceArtifactV2::Unavailable,
        ) => {
            bail!("available global provenance has unavailable option provenance")
        }
        (
            ConfigProvenanceArtifactStateV2::Unavailable { .. },
            ConfigOptionProvenanceArtifactV2::Available { .. },
        ) => {
            bail!("unavailable global provenance has available option provenance")
        }
    }
}

fn metadata_artifact(
    path_components: &[String],
    metadata: InspectionMetadata,
) -> Result<ConfigOptionMetadataArtifactV2> {
    match metadata {
        InspectionMetadata::Failed(error) => Ok(ConfigOptionMetadataArtifactV2::Failed { error }),
        InspectionMetadata::Available(metadata) => {
            if metadata.path != path_components {
                bail!("config artifact metadata path disagrees with option path");
            }
            Ok(ConfigOptionMetadataArtifactV2::Available {
                option_type: metadata.option_type,
                loc: metadata.loc,
                declared_type: metadata.declared_type,
                declarations: metadata.declarations,
                declaration_positions: metadata.declaration_positions,
                highest_prio: metadata.highest_prio,
                is_defined: metadata.is_defined,
                surviving_definition_sources: metadata
                    .surviving_definition_sources
                    .into_iter()
                    .map(|source| ConfigDefinitionSourceArtifactV2 {
                        source_path: source.source_path,
                        priority: source.priority,
                        source_input: source.source_input,
                        source_revision: source.source_revision,
                    })
                    .collect(),
            })
        }
    }
}

fn provenance_artifact(
    option_key: &str,
    provenance: &AssembledOptionProvenance,
) -> Result<ConfigOptionProvenanceArtifactV2> {
    match provenance {
        AssembledOptionProvenance::Unavailable => Ok(ConfigOptionProvenanceArtifactV2::Unavailable),
        AssembledOptionProvenance::Available {
            definitions,
            override_state,
        } => {
            let override_state = match override_state {
                OverrideState::Known(value) => *value,
            };
            let definitions = definitions
                .iter()
                .map(|definition| definition_artifact(option_key, definition))
                .collect::<Result<Vec<_>>>()?;
            validate_definition_structure(&definitions)?;
            Ok(ConfigOptionProvenanceArtifactV2::Available {
                definitions,
                override_state,
            })
        }
    }
}

fn definition_artifact(
    option_key: &str,
    definition: &AssembledDefinition,
) -> Result<ConfigDefinitionArtifactV2> {
    if definition.option_key != option_key {
        bail!("definition option key disagrees with owner");
    }
    Ok(ConfigDefinitionArtifactV2 {
        option_key: definition.option_key.clone(),
        ordinal: definition.ordinal,
        source_path: definition.source_path.clone(),
        source_input: definition.source_input.clone(),
        source_revision: definition.source_revision.clone(),
        module_key: definition.module_key.clone(),
        priority: definition.priority,
        status: match definition.status {
            RawDefinitionStatus::ActiveSurviving => ConfigDefinitionStatusV2::ActiveSurviving,
            RawDefinitionStatus::PriorityDiscarded => ConfigDefinitionStatusV2::PriorityDiscarded,
        },
        surviving_merge_order: definition.surviving_merge_order,
        value: definition
            .value
            .as_ref()
            .map(|value| inspection_value(value.clone())),
    })
}

fn enrichment_artifact(
    state: &DefinitionValueEnrichmentState,
) -> Result<DefinitionValueArtifactStateV2> {
    Ok(match state {
        DefinitionValueEnrichmentState::Available {
            adapter_version,
            provenance_digest,
        } => DefinitionValueArtifactStateV2::Available {
            adapter_version: *adapter_version,
            provenance_digest: provenance_digest.clone(),
        },
        DefinitionValueEnrichmentState::Unavailable {
            reason_code,
            diagnostic,
        } => DefinitionValueArtifactStateV2::Unavailable {
            reason_code: reason_code.clone(),
            diagnostic: diagnostic.clone(),
        },
    })
}

fn validate_definition_values(
    definitions: &[ConfigDefinitionArtifactV2],
    enrichment: &DefinitionValueArtifactStateV2,
) -> Result<()> {
    let values_available = matches!(enrichment, DefinitionValueArtifactStateV2::Available { .. });
    for definition in definitions {
        if definition.value.is_some() != values_available {
            bail!("definition value presence disagrees with enrichment state");
        }
    }
    Ok(())
}

fn validate_definition_structure(definitions: &[ConfigDefinitionArtifactV2]) -> Result<()> {
    let mut survivor_orders = BTreeSet::new();
    let mut survivor_count = 0_u64;
    for (expected_ordinal, definition) in definitions.iter().enumerate() {
        let expected_ordinal = u64::try_from(expected_ordinal)
            .context("definition count exceeds supported ordinal range")?;
        if definition.ordinal != expected_ordinal {
            bail!("artifact definition ordinals are not contiguous");
        }
        match definition.status {
            ConfigDefinitionStatusV2::ActiveSurviving => {
                let Some(order) = definition.surviving_merge_order else {
                    bail!("active artifact definition is missing merge order");
                };
                if !survivor_orders.insert(order) {
                    bail!("duplicate artifact surviving merge order");
                }
                survivor_count += 1;
            }
            ConfigDefinitionStatusV2::PriorityDiscarded => {
                if definition.surviving_merge_order.is_some() {
                    bail!("discarded artifact definition has merge order");
                }
            }
        }
    }
    if survivor_orders != (0..survivor_count).collect::<BTreeSet<_>>() {
        bail!("artifact surviving merge orders are not contiguous");
    }
    Ok(())
}

fn inspection_value(value: InspectionValue) -> SafeOptionValue {
    match value {
        InspectionValue::Available(value) => value,
        InspectionValue::Failed(error) => SafeOptionValue::Failed(error),
    }
}

fn is_key(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

// SECURITY: The dotted suffix is only policy context for the existing matcher;
// the canonical JSON prefix and the artifact path components remain authoritative.
fn redaction_context(path_components: &[String]) -> String {
    let canonical = serde_json::to_string(path_components).unwrap_or_default();
    format!("{canonical} {}", path_components.join("."))
}

fn redact_error(mut error: SafeEvaluationError) -> SafeEvaluationError {
    error.code = redact_text(&error.code);
    error.message = redact_evaluation_error(&error.message);
    error
}

fn redact_metadata(metadata: ConfigOptionMetadataArtifactV2) -> ConfigOptionMetadataArtifactV2 {
    match metadata {
        ConfigOptionMetadataArtifactV2::Failed { error } => {
            ConfigOptionMetadataArtifactV2::Failed {
                error: redact_error(error),
            }
        }
        ConfigOptionMetadataArtifactV2::Available {
            option_type,
            loc,
            declared_type,
            declarations,
            declaration_positions,
            highest_prio,
            is_defined,
            surviving_definition_sources,
        } => ConfigOptionMetadataArtifactV2::Available {
            option_type: option_type.map(|value| redact_text(&value)),
            loc: loc.into_iter().map(|value| redact_text(&value)).collect(),
            declared_type: declared_type.map(|value| redact_text(&value)),
            declarations: declarations
                .into_iter()
                .map(|value| redact_text(&value))
                .collect(),
            declaration_positions: declaration_positions
                .into_iter()
                .map(|value| redact_json(&value))
                .collect(),
            highest_prio,
            is_defined,
            surviving_definition_sources: surviving_definition_sources
                .into_iter()
                .map(|source| ConfigDefinitionSourceArtifactV2 {
                    source_path: redact_text(&source.source_path),
                    priority: source.priority,
                    source_input: source.source_input.map(|value| redact_text(&value)),
                    source_revision: source.source_revision.map(|value| redact_text(&value)),
                })
                .collect(),
        },
    }
}

fn redact_provenance(
    provenance: ConfigOptionProvenanceArtifactV2,
    context: &str,
) -> ConfigOptionProvenanceArtifactV2 {
    match provenance {
        ConfigOptionProvenanceArtifactV2::Unavailable => {
            ConfigOptionProvenanceArtifactV2::Unavailable
        }
        ConfigOptionProvenanceArtifactV2::Available {
            definitions,
            override_state,
        } => ConfigOptionProvenanceArtifactV2::Available {
            definitions: definitions
                .into_iter()
                .map(|mut definition| {
                    definition.source_path =
                        definition.source_path.map(|value| redact_text(&value));
                    definition.source_input =
                        definition.source_input.map(|value| redact_text(&value));
                    definition.source_revision =
                        definition.source_revision.map(|value| redact_text(&value));
                    definition.module_key = definition.module_key.map(|value| redact_text(&value));
                    definition.value = definition
                        .value
                        .map(|value| redact_safe_value(context, value));
                    definition
                })
                .collect(),
            override_state,
        },
    }
}

fn redact_enrichment(state: DefinitionValueArtifactStateV2) -> DefinitionValueArtifactStateV2 {
    match state {
        DefinitionValueArtifactStateV2::Available {
            adapter_version,
            provenance_digest,
        } => DefinitionValueArtifactStateV2::Available {
            adapter_version,
            provenance_digest: redact_text(&provenance_digest),
        },
        DefinitionValueArtifactStateV2::Unavailable {
            reason_code,
            diagnostic,
        } => DefinitionValueArtifactStateV2::Unavailable {
            reason_code: redact_text(&reason_code),
            diagnostic: diagnostic.map(redact_error),
        },
    }
}

fn redact_safe_value(context: &str, value: SafeOptionValue) -> SafeOptionValue {
    match value {
        SafeOptionValue::Scalar(value) => {
            SafeOptionValue::Scalar(redact_option_value(context, &value))
        }
        SafeOptionValue::Package(mut package) => {
            package.name = package.name.map(|value| redact_string(context, value));
            package.pname = package.pname.map(|value| redact_string(context, value));
            package.version = package.version.map(|value| redact_string(context, value));
            package.output_path = package
                .output_path
                .map(|value| redact_string(context, value));
            SafeOptionValue::Package(package)
        }
        SafeOptionValue::List(values) => SafeOptionValue::List(
            values
                .into_iter()
                .map(|value| redact_safe_value(context, value))
                .collect(),
        ),
        SafeOptionValue::AttributeSet(values) => {
            match redact_option_value(context, &Value::Object(values)) {
                Value::Object(values) => SafeOptionValue::AttributeSet(values),
                _ => unreachable!("redacting an object preserves its JSON kind"),
            }
        }
        SafeOptionValue::Submodule(values) => {
            match redact_option_value(context, &Value::Object(values)) {
                Value::Object(values) => SafeOptionValue::Submodule(values),
                _ => unreachable!("redacting an object preserves its JSON kind"),
            }
        }
        SafeOptionValue::Opaque { type_name } => SafeOptionValue::Opaque {
            type_name: redact_text(&type_name),
        },
        SafeOptionValue::Failed(mut error) => {
            error = redact_error(error);
            SafeOptionValue::Failed(error)
        }
    }
}

fn redact_string(context: &str, value: String) -> String {
    redact_option_value(context, &Value::String(value))
        .as_str()
        .unwrap_or(REDACTED_VALUE)
        .to_string()
}

fn option_content_projection(option: &ConfigOptionArtifactV2) -> Value {
    let metadata = match &option.metadata {
        ConfigOptionMetadataArtifactV2::Available {
            option_type,
            loc,
            declared_type,
            declarations,
            declaration_positions,
            highest_prio,
            is_defined,
            surviving_definition_sources,
        } => json!({
            "state": "available",
            "option_type": option_type,
            "loc": loc,
            "declared_type": declared_type,
            "declarations": declarations,
            "declaration_positions": declaration_positions,
            "highest_prio": highest_prio,
            "is_defined": is_defined,
            "surviving_definition_sources": surviving_definition_sources,
        }),
        ConfigOptionMetadataArtifactV2::Failed { error } => {
            json!({ "state": "failed", "error": error })
        }
    };
    let provenance = match &option.provenance {
        ConfigOptionProvenanceArtifactV2::Unavailable => json!({
            "state": "unavailable",
        }),
        ConfigOptionProvenanceArtifactV2::Available {
            definitions,
            override_state,
        } => json!({
            "state": "available",
            "definitions": definitions.iter().map(|definition| json!({
                "ordinal": definition.ordinal,
                "source_path": definition.source_path,
                "source_input": definition.source_input,
                "source_revision": definition.source_revision,
                "module_key": definition.module_key,
                "priority": definition.priority,
                "status": definition.status,
                "surviving_merge_order": definition.surviving_merge_order,
                "value": definition.value,
            })).collect::<Vec<_>>(),
            "override_state": override_state,
        }),
    };
    json!({
        "metadata": metadata,
        "effective_value": option.effective_value,
        "provenance": provenance,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::config_inspector::{
        AssembledDefinition, AssembledOption, AssembledOptionProvenance,
        DefinitionValueEnrichmentState, InspectionMetadata, InspectionValue, OptionMetadata,
        RawDefinitionStatus,
    };

    const KEY_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const KEY_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn metadata(path: &[&str]) -> InspectionMetadata {
        InspectionMetadata::Available(OptionMetadata {
            path: path.iter().map(|part| (*part).to_string()).collect(),
            option_type: Some("option".to_string()),
            loc: vec!["module.nix".to_string()],
            declared_type: Some("string".to_string()),
            declarations: vec!["/nix/store/source/module.nix".to_string()],
            declaration_positions: vec![json!({ "line": 10, "column": 2 })],
            highest_prio: Some(100),
            is_defined: true,
            surviving_definition_sources: Vec::new(),
        })
    }

    fn value(text: &str) -> InspectionValue {
        InspectionValue::Available(SafeOptionValue::Scalar(Value::String(text.to_string())))
    }

    fn definition(
        option_key: &str,
        ordinal: u64,
        status: RawDefinitionStatus,
        merge_order: Option<u64>,
        value: Option<InspectionValue>,
    ) -> AssembledDefinition {
        AssembledDefinition {
            option_key: option_key.to_string(),
            ordinal,
            source_path: Some("/nix/store/source/module.nix".to_string()),
            source_input: Some("self".to_string()),
            source_revision: Some("revision".to_string()),
            module_key: Some("module".to_string()),
            priority: 100,
            status,
            surviving_merge_order: merge_order,
            value,
        }
    }

    fn provenance(
        definitions: Vec<AssembledDefinition>,
        _enrichment: DefinitionValueEnrichmentState,
        override_state: OverrideState,
    ) -> AssembledOptionProvenance {
        AssembledOptionProvenance::Available {
            definitions,
            override_state,
        }
    }

    fn option(
        option_key: &str,
        path: &[&str],
        metadata: InspectionMetadata,
        effective_value: InspectionValue,
        provenance: AssembledOptionProvenance,
    ) -> AssembledOption {
        AssembledOption {
            option_key: option_key.to_string(),
            path_components: path.iter().map(|part| (*part).to_string()).collect(),
            metadata,
            effective_value,
            provenance,
        }
    }

    fn assembled(options: Vec<AssembledOption>) -> AssembledConfigInspection {
        assembled_with_state(
            options,
            AssembledConfigProvenanceState::Available {
                adapter_version: 1,
                target_lib_version: Some("lib".to_string()),
                target_module_system_path: Some("/nix/store/lib".to_string()),
                provenance_digest: DIGEST.to_string(),
                definition_value_enrichment: available_enrichment(),
            },
        )
    }

    fn assembled_with_state(
        options: Vec<AssembledOption>,
        provenance_state: AssembledConfigProvenanceState,
    ) -> AssembledConfigInspection {
        AssembledConfigInspection {
            target_key: KEY_A.to_string(),
            source_out_path: "/nix/store/flake-source".to_string(),
            carrier_drv_path: "/nix/store/carrier.drv".to_string(),
            provenance_state,
            options,
        }
    }

    fn available_enrichment() -> DefinitionValueEnrichmentState {
        DefinitionValueEnrichmentState::Available {
            adapter_version: 1,
            provenance_digest: DIGEST.to_string(),
        }
    }

    fn unavailable_enrichment() -> DefinitionValueEnrichmentState {
        DefinitionValueEnrichmentState::Unavailable {
            reason_code: "stage2_index_failed".to_string(),
            diagnostic: Some(SafeEvaluationError {
                code: "stage2_index_failed".to_string(),
                message: "definition values unavailable".to_string(),
            }),
        }
    }

    #[test]
    fn conversion_preserves_v2_identity_states_and_definition_semantics() {
        let path = ["foo", "bar.baz"];
        let mut discarded = definition(
            KEY_A,
            1,
            RawDefinitionStatus::PriorityDiscarded,
            None,
            Some(InspectionValue::Failed(SafeEvaluationError {
                code: "not_evaluated".to_string(),
                message: "definition failed".to_string(),
            })),
        );
        discarded.source_path = None;
        let artifact = config_artifact_v2_from_assembled(assembled(vec![option(
            KEY_A,
            &path,
            metadata(&path),
            InspectionValue::Failed(SafeEvaluationError {
                code: "not_evaluated".to_string(),
                message: "value unavailable".to_string(),
            }),
            provenance(
                vec![
                    definition(
                        KEY_A,
                        0,
                        RawDefinitionStatus::ActiveSurviving,
                        Some(0),
                        Some(value("one")),
                    ),
                    discarded,
                    definition(
                        KEY_A,
                        2,
                        RawDefinitionStatus::ActiveSurviving,
                        Some(1),
                        Some(value("two")),
                    ),
                ],
                available_enrichment(),
                OverrideState::Known(true),
            ),
        )]))
        .unwrap();

        assert_eq!(artifact.artifact_version, 2);
        assert_eq!(artifact.target_key, KEY_A);
        assert_eq!(
            artifact.options[0].path_components,
            path.iter()
                .map(|part| (*part).to_string())
                .collect::<Vec<_>>()
        );
        assert!(matches!(
            artifact.options[0].metadata,
            ConfigOptionMetadataArtifactV2::Available { .. }
        ));
        assert!(matches!(
            artifact.options[0].effective_value,
            SafeOptionValue::Failed(_)
        ));
        let ConfigOptionProvenanceArtifactV2::Available {
            definitions,
            override_state,
            ..
        } = &artifact.options[0].provenance
        else {
            panic!("expected available provenance");
        };
        assert_eq!(definitions.len(), 3);
        assert_eq!(definitions[0].surviving_merge_order, Some(0));
        assert_eq!(
            definitions[1].status,
            ConfigDefinitionStatusV2::PriorityDiscarded
        );
        assert_eq!(definitions[1].source_path, None);
        assert_eq!(definitions[2].surviving_merge_order, Some(1));
        assert_eq!(*override_state, true);
    }

    #[test]
    fn conversion_preserves_metadata_failure_and_unavailable_provenance() {
        let metadata_error = SafeEvaluationError {
            code: "metadata_failed".to_string(),
            message: "metadata failed".to_string(),
        };
        let provenance = AssembledOptionProvenance::Unavailable;
        let artifact = config_artifact_v2_from_assembled(assembled_with_state(
            vec![option(
                KEY_A,
                &["feature"],
                InspectionMetadata::Failed(metadata_error.clone()),
                value("effective"),
                provenance,
            )],
            AssembledConfigProvenanceState::Unavailable {
                reason_code: "capability_unavailable".to_string(),
                diagnostic: None,
            },
        ))
        .unwrap();
        assert_eq!(
            artifact.options[0].metadata,
            ConfigOptionMetadataArtifactV2::Failed {
                error: metadata_error
            }
        );
        assert!(matches!(
            artifact.options[0].provenance,
            ConfigOptionProvenanceArtifactV2::Unavailable { .. }
        ));
    }

    #[test]
    fn zero_definitions_and_global_enrichment_unavailability_remain_distinct() {
        let zero = config_artifact_v2_from_assembled(assembled(vec![option(
            KEY_A,
            &["feature"],
            metadata(&["feature"]),
            value("effective"),
            provenance(
                Vec::new(),
                available_enrichment(),
                OverrideState::Known(false),
            ),
        )]))
        .unwrap();
        let ConfigOptionProvenanceArtifactV2::Available {
            definitions,
            override_state,
            ..
        } = &zero.options[0].provenance
        else {
            panic!("expected available provenance");
        };
        assert!(definitions.is_empty());
        assert!(!*override_state);
        assert!(matches!(
            zero.provenance_state,
            ConfigProvenanceArtifactStateV2::Available {
                definition_value_enrichment: DefinitionValueArtifactStateV2::Available { .. },
                ..
            }
        ));

        let unavailable = config_artifact_v2_from_assembled(assembled_with_state(
            vec![option(
                KEY_A,
                &["feature"],
                metadata(&["feature"]),
                value("effective"),
                provenance(
                    vec![definition(
                        KEY_A,
                        0,
                        RawDefinitionStatus::ActiveSurviving,
                        Some(0),
                        None,
                    )],
                    unavailable_enrichment(),
                    OverrideState::Known(false),
                ),
            )],
            AssembledConfigProvenanceState::Available {
                adapter_version: 1,
                target_lib_version: Some("lib".to_string()),
                target_module_system_path: Some("/nix/store/lib".to_string()),
                provenance_digest: DIGEST.to_string(),
                definition_value_enrichment: unavailable_enrichment(),
            },
        ))
        .unwrap();
        let ConfigOptionProvenanceArtifactV2::Available { definitions, .. } =
            &unavailable.options[0].provenance
        else {
            panic!("expected available provenance");
        };
        assert_eq!(definitions[0].value, None);
        assert!(matches!(
            unavailable.provenance_state,
            ConfigProvenanceArtifactStateV2::Available {
                definition_value_enrichment: DefinitionValueArtifactStateV2::Unavailable { .. },
                ..
            }
        ));
    }

    #[test]
    fn persisted_decoder_preserves_exact_path_component_collisions() {
        let first_path = vec!["foo".to_string(), "bar.baz".to_string()];
        let second_path = vec!["foo".to_string(), "bar".to_string(), "baz".to_string()];
        let first = ConfigOptionArtifactV2 {
            option_key: canonical_option_key(&first_path),
            path_components: first_path.clone(),
            metadata: ConfigOptionMetadataArtifactV2::Failed {
                error: SafeEvaluationError {
                    code: "metadata_failed".to_string(),
                    message: "metadata unavailable".to_string(),
                },
            },
            effective_value: SafeOptionValue::Scalar(json!("first")),
            provenance: ConfigOptionProvenanceArtifactV2::Unavailable,
        };
        let second = ConfigOptionArtifactV2 {
            option_key: canonical_option_key(&second_path),
            path_components: second_path.clone(),
            metadata: first.metadata.clone(),
            effective_value: SafeOptionValue::Scalar(json!("second")),
            provenance: ConfigOptionProvenanceArtifactV2::Unavailable,
        };

        let first_decoded = config_option_v2_from_persisted(
            first.option_key.clone(),
            first_path.clone(),
            ConfigInspectionArtifactV2::option_content_payload(&first),
        )
        .expect("first exact path should decode");
        let second_decoded = config_option_v2_from_persisted(
            second.option_key.clone(),
            second_path.clone(),
            ConfigInspectionArtifactV2::option_content_payload(&second),
        )
        .expect("second exact path should decode");

        assert_ne!(first.option_key, second.option_key);
        assert_eq!(first_decoded.path_components, first_path);
        assert_eq!(second_decoded.path_components, second_path);
        assert!(
            config_option_v2_from_persisted(
                first.option_key,
                second_decoded.path_components,
                ConfigInspectionArtifactV2::option_content_payload(&first_decoded),
            )
            .is_err()
        );
    }

    #[test]
    fn conversion_rejects_impossible_definition_value_presence() {
        let result = config_artifact_v2_from_assembled(assembled_with_state(
            vec![option(
                KEY_A,
                &["feature"],
                metadata(&["feature"]),
                value("effective"),
                provenance(
                    vec![definition(
                        KEY_A,
                        0,
                        RawDefinitionStatus::ActiveSurviving,
                        Some(0),
                        Some(value("should be absent")),
                    )],
                    unavailable_enrichment(),
                    OverrideState::Known(false),
                ),
            )],
            AssembledConfigProvenanceState::Available {
                adapter_version: 1,
                target_lib_version: Some("lib".to_string()),
                target_module_system_path: Some("/nix/store/lib".to_string()),
                provenance_digest: DIGEST.to_string(),
                definition_value_enrichment: unavailable_enrichment(),
            },
        ));
        assert!(result.is_err());
    }

    #[test]
    fn exact_paths_are_preserved_and_content_deduplicates_across_paths() {
        let first_path = ["foo", "bar.baz"];
        let second_path = ["foo", "bar", "baz"];
        let first = config_artifact_v2_from_assembled(assembled(vec![option(
            KEY_A,
            &first_path,
            metadata(&first_path),
            value("same"),
            provenance(
                Vec::new(),
                available_enrichment(),
                OverrideState::Known(false),
            ),
        )]))
        .unwrap();
        let second = config_artifact_v2_from_assembled(assembled(vec![option(
            KEY_B,
            &second_path,
            metadata(&second_path),
            value("same"),
            provenance(
                Vec::new(),
                available_enrichment(),
                OverrideState::Known(false),
            ),
        )]))
        .unwrap();
        assert_ne!(
            first.options[0].path_components,
            second.options[0].path_components
        );
        assert_eq!(
            ConfigInspectionArtifactV2::option_content_digest(&first.options[0]),
            ConfigInspectionArtifactV2::option_content_digest(&second.options[0])
        );
    }

    #[test]
    fn redaction_and_search_use_safe_content_only() {
        let path = ["services", "password"];
        let artifact = config_artifact_v2_from_assembled(assembled(vec![option(
            KEY_A,
            &path,
            InspectionMetadata::Available(OptionMetadata {
                path: path.iter().map(|part| (*part).to_string()).collect(),
                option_type: Some("option".to_string()),
                loc: Vec::new(),
                declared_type: Some("password".to_string()),
                declarations: vec![
                    "https://user:secret@example.com/repo?token=query-secret".to_string(),
                ],
                declaration_positions: vec![json!({ "error": "password=metadata-secret" })],
                highest_prio: Some(100),
                is_defined: true,
                surviving_definition_sources: Vec::new(),
            }),
            value("password=effective-secret"),
            provenance(
                vec![definition(
                    KEY_A,
                    0,
                    RawDefinitionStatus::ActiveSurviving,
                    Some(0),
                    Some(value("token=definition-secret")),
                )],
                available_enrichment(),
                OverrideState::Known(false),
            ),
        )]))
        .unwrap()
        .redacted();
        let serialized = serde_json::to_string(&artifact).unwrap();
        for secret in [
            "effective-secret",
            "definition-secret",
            "metadata-secret",
            "query-secret",
            "user:secret",
        ] {
            assert!(!serialized.contains(secret), "secret leaked: {secret}");
        }
        let search = ConfigInspectionArtifactV2::option_search_text(&artifact.options[0]);
        assert!(search.len() <= MAX_SEARCH_TEXT_CHARS);
        assert!(!search.contains("effective-secret"));
        assert!(!search.contains("services"));
    }

    #[test]
    fn content_digest_changes_for_semantic_content_and_state_changes() {
        let base = config_artifact_v2_from_assembled(assembled(vec![option(
            KEY_A,
            &["feature"],
            metadata(&["feature"]),
            value("one"),
            provenance(
                Vec::new(),
                available_enrichment(),
                OverrideState::Known(false),
            ),
        )]))
        .unwrap()
        .redacted();
        let changed_value = config_artifact_v2_from_assembled(assembled(vec![option(
            KEY_A,
            &["feature"],
            metadata(&["feature"]),
            value("two"),
            provenance(
                Vec::new(),
                available_enrichment(),
                OverrideState::Known(false),
            ),
        )]))
        .unwrap()
        .redacted();
        let changed_status = config_artifact_v2_from_assembled(assembled(vec![option(
            KEY_A,
            &["feature"],
            metadata(&["feature"]),
            value("one"),
            provenance(
                vec![definition(
                    KEY_A,
                    0,
                    RawDefinitionStatus::PriorityDiscarded,
                    None,
                    Some(value("definition")),
                )],
                available_enrichment(),
                OverrideState::Known(true),
            ),
        )]))
        .unwrap()
        .redacted();
        let unavailable_provenance = config_artifact_v2_from_assembled(assembled_with_state(
            vec![option(
                KEY_A,
                &["feature"],
                metadata(&["feature"]),
                value("one"),
                AssembledOptionProvenance::Unavailable,
            )],
            AssembledConfigProvenanceState::Unavailable {
                reason_code: "unavailable".to_string(),
                diagnostic: None,
            },
        ))
        .unwrap()
        .redacted();
        let unavailable_values = config_artifact_v2_from_assembled(assembled_with_state(
            vec![option(
                KEY_A,
                &["feature"],
                metadata(&["feature"]),
                value("one"),
                provenance(
                    vec![definition(
                        KEY_A,
                        0,
                        RawDefinitionStatus::ActiveSurviving,
                        Some(0),
                        None,
                    )],
                    unavailable_enrichment(),
                    OverrideState::Known(false),
                ),
            )],
            AssembledConfigProvenanceState::Available {
                adapter_version: 1,
                target_lib_version: Some("lib".to_string()),
                target_module_system_path: Some("/nix/store/lib".to_string()),
                provenance_digest: DIGEST.to_string(),
                definition_value_enrichment: unavailable_enrichment(),
            },
        ))
        .unwrap()
        .redacted();
        let digest = |artifact: &ConfigInspectionArtifactV2| {
            ConfigInspectionArtifactV2::option_content_digest(&artifact.options[0])
        };
        assert_ne!(digest(&base), digest(&changed_value));
        assert_ne!(digest(&base), digest(&changed_status));
        assert_ne!(digest(&base), digest(&unavailable_provenance));
        assert_ne!(digest(&base), digest(&unavailable_values));
    }

    #[test]
    fn option_content_digest_excludes_global_provenance_identity() {
        let make_option = |key, path, text| {
            option(
                key,
                path,
                metadata(path),
                value(text),
                provenance(
                    Vec::new(),
                    available_enrichment(),
                    OverrideState::Known(false),
                ),
            )
        };
        let first = config_artifact_v2_from_assembled(assembled_with_state(
            vec![
                make_option(KEY_A, &["feature", "a"], "one"),
                make_option(KEY_B, &["feature", "b"], "one"),
            ],
            AssembledConfigProvenanceState::Available {
                adapter_version: 1,
                target_lib_version: Some("lib-a".to_string()),
                target_module_system_path: Some("/nix/store/lib-a".to_string()),
                provenance_digest:
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                definition_value_enrichment: available_enrichment(),
            },
        ))
        .unwrap();
        let second = config_artifact_v2_from_assembled(assembled_with_state(
            vec![
                make_option(KEY_A, &["feature", "a"], "one"),
                make_option(KEY_B, &["feature", "b"], "two"),
            ],
            AssembledConfigProvenanceState::Available {
                adapter_version: 2,
                target_lib_version: Some("lib-b".to_string()),
                target_module_system_path: Some("/nix/store/lib-b".to_string()),
                provenance_digest:
                    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                definition_value_enrichment: available_enrichment(),
            },
        ))
        .unwrap();

        assert_eq!(
            ConfigInspectionArtifactV2::option_content_digest(&first.options[0]),
            ConfigInspectionArtifactV2::option_content_digest(&second.options[0])
        );
        assert_ne!(
            ConfigInspectionArtifactV2::option_content_digest(&first.options[1]),
            ConfigInspectionArtifactV2::option_content_digest(&second.options[1])
        );
    }

    #[test]
    fn comparison_readiness_requires_global_provenance_and_available_enrichment() {
        let available = assembled(vec![option(
            KEY_A,
            &["feature"],
            metadata(&["feature"]),
            value("one"),
            provenance(
                Vec::new(),
                available_enrichment(),
                OverrideState::Known(false),
            ),
        )]);
        assert!(
            config_artifact_v2_from_assembled(available)
                .unwrap()
                .comparison_ready()
        );

        let failed_value = InspectionValue::Failed(SafeEvaluationError {
            code: "not_evaluated".to_string(),
            message: "value unavailable".to_string(),
        });
        let failed_semantics = config_artifact_v2_from_assembled(assembled_with_state(
            vec![option(
                KEY_A,
                &["feature"],
                metadata(&["feature"]),
                failed_value.clone(),
                provenance(
                    vec![definition(
                        KEY_A,
                        0,
                        RawDefinitionStatus::ActiveSurviving,
                        Some(0),
                        Some(failed_value),
                    )],
                    available_enrichment(),
                    OverrideState::Known(false),
                ),
            )],
            AssembledConfigProvenanceState::Available {
                adapter_version: 1,
                target_lib_version: Some("lib".to_string()),
                target_module_system_path: Some("/nix/store/lib".to_string()),
                provenance_digest: DIGEST.to_string(),
                definition_value_enrichment: available_enrichment(),
            },
        ))
        .unwrap();
        assert!(failed_semantics.comparison_ready());

        let unavailable = assembled_with_state(
            Vec::new(),
            AssembledConfigProvenanceState::Unavailable {
                reason_code: "capability_unavailable".to_string(),
                diagnostic: None,
            },
        );
        assert!(
            !config_artifact_v2_from_assembled(unavailable)
                .unwrap()
                .comparison_ready()
        );
    }

    #[test]
    fn each_parent_global_field_is_excluded_from_option_digest() {
        let make_option = || {
            option(
                KEY_A,
                &["feature"],
                metadata(&["feature"]),
                value("one"),
                provenance(
                    Vec::new(),
                    available_enrichment(),
                    OverrideState::Known(false),
                ),
            )
        };
        let available = |adapter_version,
                         target_lib_version,
                         target_module_system_path,
                         provenance_digest,
                         definition_value_enrichment| {
            AssembledConfigProvenanceState::Available {
                adapter_version,
                target_lib_version,
                target_module_system_path,
                provenance_digest,
                definition_value_enrichment,
            }
        };
        let base = config_artifact_v2_from_assembled(assembled_with_state(
            vec![make_option()],
            available(
                1,
                Some("lib".to_string()),
                Some("/nix/store/lib".to_string()),
                DIGEST.to_string(),
                available_enrichment(),
            ),
        ))
        .unwrap();
        let base_digest = ConfigInspectionArtifactV2::option_content_digest(&base.options[0]);
        for state in [
            available(
                2,
                Some("lib".to_string()),
                Some("/nix/store/lib".to_string()),
                DIGEST.to_string(),
                available_enrichment(),
            ),
            available(
                1,
                Some("other-lib".to_string()),
                Some("/nix/store/lib".to_string()),
                DIGEST.to_string(),
                available_enrichment(),
            ),
            available(
                1,
                Some("lib".to_string()),
                Some("/nix/store/other-lib".to_string()),
                DIGEST.to_string(),
                available_enrichment(),
            ),
            available(
                1,
                Some("lib".to_string()),
                Some("/nix/store/lib".to_string()),
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                available_enrichment(),
            ),
            available(
                1,
                Some("lib".to_string()),
                Some("/nix/store/lib".to_string()),
                DIGEST.to_string(),
                unavailable_enrichment(),
            ),
        ] {
            let artifact =
                config_artifact_v2_from_assembled(assembled_with_state(vec![make_option()], state))
                    .unwrap();
            assert_eq!(
                base_digest,
                ConfigInspectionArtifactV2::option_content_digest(&artifact.options[0])
            );
        }
    }

    #[test]
    fn option_content_digest_includes_the_v2_schema_domain() {
        let artifact = config_artifact_v2_from_assembled(assembled(vec![option(
            KEY_A,
            &["feature"],
            metadata(&["feature"]),
            value("one"),
            provenance(
                Vec::new(),
                available_enrichment(),
                OverrideState::Known(false),
            ),
        )]))
        .unwrap();
        let option = &artifact.options[0];
        let redacted = redact_option(option.clone());
        let local = option_content_projection(&redacted);
        let raw_unversioned: [u8; 32] = Sha256::digest(serde_json::to_vec(&local).unwrap()).into();
        let expected_envelope = json!({
            "schema_version": CONFIG_OPTION_ARTIFACT_SCHEMA_VERSION_V2,
            "content": local,
        });
        let expected_v2: [u8; 32] =
            Sha256::digest(serde_json::to_vec(&expected_envelope).unwrap()).into();

        assert_ne!(
            ConfigInspectionArtifactV2::option_content_digest(option),
            raw_unversioned
        );
        assert_eq!(
            ConfigInspectionArtifactV2::option_content_digest(option),
            expected_v2
        );
    }

    #[test]
    fn available_and_unavailable_zero_option_artifacts_remain_distinct() {
        let available = config_artifact_v2_from_assembled(assembled_with_state(
            Vec::new(),
            AssembledConfigProvenanceState::Available {
                adapter_version: 1,
                target_lib_version: Some("lib".to_string()),
                target_module_system_path: Some("/nix/store/lib".to_string()),
                provenance_digest: DIGEST.to_string(),
                definition_value_enrichment: available_enrichment(),
            },
        ))
        .unwrap();
        assert!(available.options.is_empty());
        assert!(matches!(
            available.provenance_state,
            ConfigProvenanceArtifactStateV2::Available { .. }
        ));
        assert!(available.comparison_ready());

        let unavailable = config_artifact_v2_from_assembled(assembled_with_state(
            Vec::new(),
            AssembledConfigProvenanceState::Unavailable {
                reason_code: "capability_unavailable".to_string(),
                diagnostic: None,
            },
        ))
        .unwrap();
        assert!(unavailable.options.is_empty());
        assert!(matches!(
            unavailable.provenance_state,
            ConfigProvenanceArtifactStateV2::Unavailable { .. }
        ));
        assert!(!unavailable.comparison_ready());
    }
}
