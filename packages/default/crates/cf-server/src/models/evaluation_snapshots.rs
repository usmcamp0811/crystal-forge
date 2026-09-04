//! Defines safe, revision-specific evaluation snapshot data.
//!
//! The types in this module are the persistence boundary for evaluator output.
//! Callers must construct [`EvaluatedOption::redacted`] before they calculate a
//! digest, build search text, persist a value, or return a value from an API.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::security::snapshot_redaction::{
    REDACTED_VALUE, redact_evaluation_error, redact_option_value, redact_text,
};

/// Identifies the durable lifecycle of an evaluation or flake-output snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotLifecycle {
    /// An authorized request has queued evaluation work.
    Queued,
    /// The existing evaluation worker is extracting the snapshot.
    Running,
    /// Evaluation ended with a safe persisted diagnostic.
    Failed,
    /// The complete snapshot is available for database-only reads.
    Available,
    /// No reusable snapshot exists for the requested revision.
    Unavailable,
}

/// Selects the revision baseline used by the options endpoint.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotRevisionMode {
    /// Compares a revision with its retained Git first-parent snapshot.
    #[default]
    Commit,
    /// Compares a retained generation with the preceding retained generation.
    Generation,
}

/// Selects the option subset returned by a bounded query.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluatedOptionFilter {
    /// Returns all options that match the search text.
    #[default]
    All,
    /// Returns options with proven lower-priority definitions.
    Overridden,
    /// Returns options whose payload differs from the valid baseline.
    Changed,
}

/// Defines bounded server-side option search and pagination.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EvaluatedOptionsParams {
    /// Full immutable commit SHA in commit mode. Prefixes are not accepted.
    #[serde(default)]
    pub revision: String,
    /// Optional retained generation for generation-mode comparison.
    pub generation: Option<i32>,
    /// Revision comparison mode.
    #[serde(default)]
    pub mode: SnapshotRevisionMode,
    /// Case-insensitive search over pre-redacted indexed text.
    #[serde(default)]
    pub search: String,
    /// Active result filter.
    #[serde(default)]
    pub filter: EvaluatedOptionFilter,
    /// Requested page size. The server clamps this value to 100.
    pub limit: Option<i64>,
    /// Zero-based result offset. The server clamps this value to 100,000.
    pub offset: Option<i64>,
    /// Opaque immutable artifact token returned with page one.
    pub snapshot_token: Option<String>,
}

/// Selects the database-only summary for one system revision.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SelectedEvaluationSummaryParams {
    /// Full immutable commit SHA in commit mode. Prefixes are not accepted.
    #[serde(default)]
    pub revision: String,
    /// Retained generation identity in generation mode.
    pub generation: Option<i32>,
    /// Revision selection mode.
    #[serde(default)]
    pub mode: SnapshotRevisionMode,
    /// Opaque artifact token from another Config response for this selection.
    pub snapshot_token: Option<String>,
}

/// Selects a bounded page of module sources for one system revision.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EvaluationModuleSourcesParams {
    /// Full immutable commit SHA in commit mode. Prefixes are not accepted.
    #[serde(default)]
    pub revision: String,
    /// Retained generation identity in generation mode.
    pub generation: Option<i32>,
    /// Revision selection mode.
    #[serde(default)]
    pub mode: SnapshotRevisionMode,
    /// Requested page size. The server clamps this value to 100.
    pub limit: Option<i64>,
    /// Zero-based source offset. The server clamps this value to 100,000.
    pub offset: Option<i64>,
    /// Opaque snapshot token returned with page one. Continuations must send it.
    pub snapshot_token: Option<String>,
}

/// Selects a bounded page from each flake-output collection.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FlakeOutputParams {
    /// Maximum rows from each top-level collection. The server clamps to 100.
    pub limit: Option<usize>,
    /// Zero-based offset applied to top-level collections.
    pub offset: Option<usize>,
    /// Opaque snapshot token. Token-aware continuations send page one's token.
    /// Tokenless positive offsets retain the endpoint's compatibility behavior.
    pub snapshot_token: Option<String>,
    /// Optional reconciliation subset. Totals remain revision-global.
    #[serde(default)]
    pub system_filter: FlakeSystemFilter,
}

/// Selects one authoritative flake-system reconciliation subset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlakeSystemFilter {
    /// Returns every visible reconciliation row.
    #[default]
    All,
    /// Returns declarations that have no visible managed system.
    DeclaredUnmanaged,
    /// Returns visible managed systems whose configuration is not declared.
    ManagedUndeclared,
}

/// Selects one bounded page of declarations for an exported module.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FlakeModuleDeclarationsParams {
    /// Requested page size. The server clamps this value to 100.
    pub limit: Option<usize>,
    /// Zero-based declaration offset. The server clamps this value to 100,000.
    pub offset: Option<usize>,
    /// Content digest returned with page one. Continuation requests must send it.
    pub snapshot_token: Option<String>,
}

/// Reports revision-global option counts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluatedOptionCounts {
    /// Number of options in the selected snapshot.
    pub all: i64,
    /// Number of options with proven overridden definitions.
    pub overridden: i64,
    /// Number of options changed from the valid baseline.
    pub changed: Option<i64>,
}

/// Returns one bounded page of revision-specific evaluated options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluatedOptionsPage {
    /// Selected snapshot lifecycle.
    pub lifecycle: SnapshotLifecycle,
    /// Full selected revision SHA.
    pub revision: String,
    /// Selected local generation identity in generation mode.
    #[serde(default)]
    pub generation: Option<i32>,
    /// Durable generation-snapshot identity, when the generation is retained.
    #[serde(default)]
    pub generation_snapshot_id: Option<Uuid>,
    /// Opaque token for the exact immutable selected artifact.
    #[serde(default)]
    pub snapshot_token: Option<String>,
    /// Full baseline SHA, or `None` when comparison is unavailable.
    pub baseline_revision: Option<String>,
    /// Preceding retained generation used as the baseline in generation mode.
    #[serde(default)]
    pub baseline_generation: Option<i32>,
    /// True when Changed has a valid baseline.
    pub comparison_available: bool,
    /// Safe evaluation error for a failed snapshot.
    pub error: Option<String>,
    /// Number of distinct `(source_input, source_revision, source_path)` tuples.
    pub module_count: i64,
    /// End-to-end evaluator duration for the selected commit, in milliseconds.
    pub evaluation_duration_ms: Option<i64>,
    /// Revision-global counts independent of search and active filter.
    pub counts: EvaluatedOptionCounts,
    /// Number of rows matching the active search and filter.
    pub total: i64,
    /// Bounded zero-based offset.
    pub offset: i64,
    /// Bounded page size.
    pub limit: i64,
    /// Evaluated option rows for this page.
    pub options: Vec<EvaluatedOptionRow>,
}

/// Identifies an active registered flake and one exact active revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackedFlakeIdentity {
    /// Registered flake database identity.
    pub flake_id: i32,
    /// Registered flake display name.
    pub flake_name: String,
    /// Registered repository URL after visibility filtering and credential sanitization.
    pub repo_url: String,
    /// Full immutable revision that exists in the registered flake timeline.
    pub revision: String,
}

/// Aggregates one exact module source across the complete selected snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationModuleSummary {
    /// Flake input name emitted by the evaluator, when known.
    pub source_input: Option<String>,
    /// Full source revision emitted by the evaluator, when known.
    pub source_revision: Option<String>,
    /// Exact source path emitted by the Nix module system.
    pub source_path: String,
    /// Number of option definitions emitted by this source.
    pub defined_count: i64,
    /// Number of options for which this source supplied the winning definition.
    pub won_count: i64,
    /// Server-issued navigation identity after exact repository and visibility checks.
    pub tracked_flake: Option<TrackedFlakeIdentity>,
}

/// Returns one stable bounded page of exact evaluation module sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationModuleSourcesPage {
    /// Selected snapshot lifecycle.
    pub lifecycle: SnapshotLifecycle,
    /// Full selected revision SHA.
    pub revision: String,
    /// Selected retained generation, when generation mode is active.
    pub generation: Option<i32>,
    /// Safe lifecycle or integrity diagnostic.
    pub error: Option<String>,
    /// Opaque token derived from the persisted snapshot identity and version.
    pub snapshot_token: Option<String>,
    /// Authoritative number of distinct source tuples in the complete snapshot.
    pub total: i64,
    /// Applied zero-based offset, clamped to 100,000.
    pub offset: i64,
    /// Applied page size, clamped to 100.
    pub limit: i64,
    /// Source tuples ordered by winning count descending, definition count
    /// descending, then input, revision, and path in ascending bytewise order
    /// with null input and revision values last.
    pub sources: Vec<EvaluationModuleSummary>,
}

/// Classifies selected-versus-running configuration drift by exact store identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationDrift {
    /// The selected and running store paths are exactly equal.
    Matches,
    /// Both store paths are known and are not equal.
    Differs,
    /// One or both exact store paths are unavailable.
    Unavailable,
}

/// Classifies the selected configuration against agent-reported store identity.
///
/// The compatibility name refers only to the agent-reported configuration store
/// path. It never identifies an agent binary, device, or hardware fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentFingerprintStatus {
    /// The selected and latest agent-reported running store paths are equal.
    Matches,
    /// Both exact store paths are available and differ.
    Differs,
    /// Either exact store path is unavailable.
    Unavailable,
}

/// Classifies exact running-store observations during the trailing seven days.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SevenDayDriftStatus {
    /// Complete observation coverage contains only the selected store path.
    NoObservedDrift,
    /// Complete observation coverage contains another exact store path.
    ObservedDrift,
    /// Coverage is absent or has a gap beyond the system offline threshold.
    InsufficientCoverage,
}

/// Summarizes one selected evaluation from persisted database state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedEvaluationSummary {
    /// Selected snapshot lifecycle.
    pub lifecycle: SnapshotLifecycle,
    /// Full selected revision SHA.
    pub revision: String,
    /// Selected retained generation, when generation mode is active.
    pub generation: Option<i32>,
    /// Safe lifecycle or integrity diagnostic.
    pub error: Option<String>,
    /// Opaque token for the exact immutable selected artifact.
    pub snapshot_token: Option<String>,
    /// Preceding retained generation used as the baseline in generation mode.
    #[serde(default)]
    pub baseline_generation: Option<i32>,
    /// Authoritative number of distinct source input, revision, and path tuples.
    pub module_source_total: i64,
    /// Snapshot completion time, when evaluation completed.
    pub completed_at: Option<DateTime<Utc>>,
    /// End-to-end evaluation duration in milliseconds.
    pub evaluation_duration_ms: Option<i64>,
    /// Authoritative option count recorded for the complete snapshot.
    pub option_total: i64,
    /// Exact selected NixOS toplevel store path, when derivation data supplies it.
    pub selected_store_path: Option<String>,
    /// Number of packages in the selected closure, when already calculated.
    pub closure_package_count: Option<i32>,
    /// Recursive Nix closure size in bytes, counting each reported store path once.
    /// `None` means no complete local Nix measurement has been persisted.
    pub closure_size_bytes: Option<i64>,
    /// Exact store path most recently reported by the running system.
    pub running_store_path: Option<String>,
    /// Agent-reported current-profile match for the latest system state.
    pub running_profile_matches: Option<bool>,
    /// Materialized number of selected option states that differ from the
    /// deterministic same-commit modal state. Absence is a state; ties use
    /// bytewise content identity. The content digest covers the complete safe
    /// option state, so definition-provenance differences count. A usable
    /// one-configuration corpus yields zero.
    pub host_delta_count: Option<i64>,
    /// Exact selected-versus-latest-agent store identity status.
    pub agent_fingerprint: AgentFingerprintStatus,
    /// Exact running-store drift over the trailing seven days.
    pub seven_day_drift: SevenDayDriftStatus,
    /// Authoritative exact-store-identity drift classification.
    pub drift: EvaluationDrift,
}

/// Adds mode-defined comparison data to one evaluated option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluatedOptionRow {
    /// Selected revision value and provenance, or `None` when removed.
    pub option: Option<EvaluatedOption>,
    /// Baseline option when comparison is available and the path existed.
    pub before: Option<EvaluatedOption>,
    /// True when selected and baseline payloads differ.
    pub changed: Option<bool>,
    /// Type-aware change summary, or `None` without a comparison baseline.
    pub diff: Option<TypedOptionDiff>,
}

/// Classifies an option-level comparison without collapsing removal into absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionChangeKind {
    /// The option exists only in the selected snapshot.
    Added,
    /// The option exists only in the baseline snapshot.
    Removed,
    /// The option exists in both snapshots with different typed content.
    Modified,
    /// The option content is unchanged.
    Unchanged,
}

/// Describes typed value additions and removals for one option comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypedOptionDiff {
    /// Option-level change classification.
    pub kind: OptionChangeKind,
    /// Value kind used for presentation, such as `package` or `list`.
    pub value_kind: String,
    /// Added package identities, collection elements, attributes, or scalar value.
    pub added: Vec<Value>,
    /// Removed package identities, collection elements, attributes, or scalar value.
    pub removed: Vec<Value>,
}

/// Reports whether an explicit evaluation action queued or reused work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueEvaluationResponse {
    /// Full requested revision SHA.
    pub revision: String,
    /// Current lifecycle after the idempotent action.
    pub lifecycle: SnapshotLifecycle,
    /// True only when this request changed the commit to queued.
    pub queued: bool,
}

/// Represents one system in authoritative flake reconciliation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciledFlakeSystem {
    /// Declared NixOS configuration name or managed configuration name.
    pub configuration_name: String,
    /// Managed Crystal Forge system, when one exists.
    pub system_id: Option<Uuid>,
    /// Managed hostname, when one exists.
    pub hostname: Option<String>,
    /// Visible managed environment name. Unmanaged declarations have no environment.
    pub environment_name: Option<String>,
    /// Visible managed environment color in the persisted UI color format.
    pub environment_color: Option<String>,
    /// Relationship between declared output and managed system.
    pub state: ReconciledFlakeSystemState,
    /// Full deployed revision when it differs from the selected revision.
    pub deployed_revision: Option<String>,
    /// True when multiple managed hosts collapse onto this output name.
    pub output_collapsed: bool,
}

/// Classifies the authoritative declared-to-managed system relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciledFlakeSystemState {
    /// The selected revision declares a managed configuration.
    Managed,
    /// The selected revision declares a configuration with no managed system.
    DeclaredUnmanaged,
    /// A managed system references a configuration absent from the revision.
    ManagedUndeclared,
}

/// Returns cached revision-scoped flake outputs and reconciliation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeOutputSnapshotResponse {
    /// Explicit snapshot lifecycle.
    pub lifecycle: SnapshotLifecycle,
    /// Full selected revision SHA.
    pub revision: String,
    /// Full Git first-parent revision, when known.
    pub first_parent_revision: Option<String>,
    /// True when Git authoritatively resolved parent data. A true value with no
    /// parent revision identifies a root commit.
    pub first_parent_resolved: bool,
    /// True only when the first-parent output snapshot is available.
    pub comparison_available: bool,
    /// Safe failure diagnostic.
    pub error: Option<String>,
    /// Opaque token bound to the selected output and first-parent comparison state.
    /// Token-aware continuations present this token to detect replacement.
    pub snapshot_token: Option<String>,
    /// Redacted selected-revision output payload.
    ///
    /// Exported-module `source_*` fields identify only the `nixosModules`
    /// attribute binding reported by Nix. They are not value provenance and do
    /// not authorize or imply source navigation. Input
    /// `transitive_descendant_count` is the complete descendant count;
    /// `direct_descendant_count` is retained for compatibility and counts only
    /// immediate children.
    pub outputs: Option<Value>,
    /// Redacted first-parent output payload when comparison is available.
    pub previous_outputs: Option<Value>,
    /// Typed first-parent output delta, or `None` when no baseline is available.
    pub delta: Option<FlakeOutputDelta>,
    /// Authoritative declared-to-managed system reconciliation.
    pub systems: Vec<ReconciledFlakeSystem>,
    /// Total number of visible active managed systems for this flake. This is
    /// revision-independent and can exceed the bounded `systems` page.
    pub managed_system_count: i64,
    /// Number of declared configurations in the selected revision.
    pub declared_system_count: i64,
    /// Number of declared configurations in the usable Git first parent.
    /// `None` means no authoritative comparison snapshot is available. The
    /// design defines no numeric output-collapse threshold, so callers must not
    /// infer this value from bounded delta samples.
    pub previous_declared_system_count: Option<i64>,
    /// Revision-global count of visible declared-but-unmanaged rows.
    pub declared_unmanaged_count: i64,
    /// Revision-global count of visible managed-but-undeclared rows.
    pub managed_undeclared_count: i64,
    /// Revision-global count of visible managed rows sharing an output name.
    pub output_collapsed_count: i64,
    /// Revision-global count of visible managed systems pinned away from the selected revision.
    pub pinned_revision_count: i64,
    /// Revision-global count of direct inputs older than 90 days.
    pub stale_direct_input_count: i64,
    /// Number of exported modules in the selected revision before pagination.
    pub exported_module_count: i64,
    /// Pagination metadata for bounded top-level output collections.
    pub pagination: FlakeOutputPagination,
}

/// Describes bounded flake-output collection paging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlakeOutputPagination {
    /// Applied zero-based offset.
    pub offset: usize,
    /// Applied per-collection limit.
    pub limit: usize,
    /// Number of visible rows after the selected reconciliation filter.
    pub system_total: i64,
    /// True when another reconciliation row exists after this page.
    pub systems_has_more: bool,
}

/// Represents one safe declaration from an exported `nixosModules` output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeModuleDeclaration {
    /// Declared option path.
    pub path: String,
    /// Declared Nix option type.
    pub declared_type: String,
    /// Whether the evaluated declaration has a safe default.
    pub has_default: bool,
    /// Safe default value when present.
    pub default: Option<Value>,
    /// Complete declaration source paths.
    pub source_paths: Vec<String>,
}

/// Returns one stable page of declarations for an exported module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlakeModuleDeclarationsPage {
    /// Selected flake-output snapshot lifecycle.
    pub lifecycle: SnapshotLifecycle,
    /// Full selected revision SHA.
    pub revision: String,
    /// Exact exported module name.
    pub module_name: String,
    /// Safe evaluation or integrity diagnostic for a non-available snapshot.
    pub error: Option<String>,
    /// Hex-encoded content digest that identifies the complete JSONB snapshot.
    pub snapshot_token: Option<String>,
    /// Authoritative declaration count for the module.
    pub total: i64,
    /// Applied zero-based offset.
    pub offset: usize,
    /// Applied page limit, clamped to 100.
    pub limit: usize,
    /// Declarations in deterministic stable order.
    pub declarations: Vec<FlakeModuleDeclaration>,
}

/// Summarizes revision-scoped flake output changes against the Git first parent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlakeOutputDelta {
    /// Exact number of added declared configurations before sample truncation.
    pub systems_added_total: usize,
    /// Exact number of removed declared configurations before sample truncation.
    pub systems_removed_total: usize,
    /// Exact number of added exported modules before sample truncation.
    pub modules_added_total: usize,
    /// Exact number of removed exported modules before sample truncation.
    pub modules_removed_total: usize,
    /// Exact number of added resolved lock nodes before sample truncation.
    pub inputs_added_total: usize,
    /// Exact number of removed resolved lock nodes before sample truncation.
    pub inputs_removed_total: usize,
    /// Exact number of resolved lock revision changes before sample truncation.
    pub input_revision_bumps_total: usize,
    /// Declared NixOS configurations added by the selected revision.
    pub systems_added: Vec<String>,
    /// Declared NixOS configurations removed by the selected revision.
    pub systems_removed: Vec<String>,
    /// Exported module names added by the selected revision.
    pub modules_added: Vec<String>,
    /// Exported module names removed by the selected revision.
    pub modules_removed: Vec<String>,
    /// Stable lock node identities added by the selected revision.
    pub inputs_added: Vec<String>,
    /// Stable lock node identities removed by the selected revision.
    pub inputs_removed: Vec<String>,
    /// Resolved lock inputs whose locked revision changed.
    pub input_revision_bumps: Vec<FlakeInputRevisionBump>,
}

/// Describes one resolved input revision change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlakeInputRevisionBump {
    /// Stable lock node identity.
    pub node: String,
    /// Previous full locked revision, when present.
    pub before: Option<String>,
    /// Selected full locked revision, when present.
    pub after: Option<String>,
}

/// Computes flake output deltas from persisted, already-redacted payloads.
pub fn flake_output_delta(before: &Value, after: &Value) -> FlakeOutputDelta {
    use std::collections::{BTreeMap, BTreeSet};

    fn strings(value: Option<&Value>) -> BTreeSet<String> {
        value
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    }
    fn module_names(value: &Value) -> BTreeSet<String> {
        value
            .get("exported_modules")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|module| module.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .collect()
    }
    fn input_revisions(value: &Value) -> BTreeMap<String, Option<String>> {
        value
            .get("inputs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|input| {
                let node = input.get("node")?.as_str()?.to_string();
                let revision = input
                    .get("locked_revision")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                Some((node, revision))
            })
            .collect()
    }

    let before_systems = strings(before.get("declared_systems"));
    let after_systems = strings(after.get("declared_systems"));
    let before_modules = module_names(before);
    let after_modules = module_names(after);
    let before_inputs = input_revisions(before);
    let after_inputs = input_revisions(after);
    let input_revision_bumps: Vec<FlakeInputRevisionBump> = before_inputs
        .keys()
        .filter(|node| after_inputs.contains_key(*node))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|node| {
            let before = before_inputs.get(node).cloned().flatten();
            let after = after_inputs.get(node).cloned().flatten();
            (before != after).then(|| FlakeInputRevisionBump {
                node: node.clone(),
                before,
                after,
            })
        })
        .collect();

    let systems_added: Vec<_> = after_systems.difference(&before_systems).cloned().collect();
    let systems_removed: Vec<_> = before_systems.difference(&after_systems).cloned().collect();
    let modules_added: Vec<_> = after_modules.difference(&before_modules).cloned().collect();
    let modules_removed: Vec<_> = before_modules.difference(&after_modules).cloned().collect();
    let inputs_added: Vec<_> = after_inputs
        .keys()
        .filter(|node| !before_inputs.contains_key(*node))
        .cloned()
        .collect();
    let inputs_removed: Vec<_> = before_inputs
        .keys()
        .filter(|node| !after_inputs.contains_key(*node))
        .cloned()
        .collect();

    FlakeOutputDelta {
        systems_added_total: systems_added.len(),
        systems_removed_total: systems_removed.len(),
        modules_added_total: modules_added.len(),
        modules_removed_total: modules_removed.len(),
        inputs_added_total: inputs_added.len(),
        inputs_removed_total: inputs_removed.len(),
        input_revision_bumps_total: input_revision_bumps.len(),
        systems_added,
        systems_removed,
        modules_added,
        modules_removed,
        inputs_added,
        inputs_removed,
        input_revision_bumps,
    }
}

/// Represents an evaluated option value without fabricating unsupported data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SafeOptionValue {
    /// A JSON scalar. All scalar types use the placeholder in sensitive contexts.
    Scalar(Value),
    /// Package identity and output metadata.
    Package(SafePackageValue),
    /// A complete list whose elements use the same tagged representation.
    /// Over-limit lists are represented by [`Self::Opaque`].
    List(Vec<SafeOptionValue>),
    /// A complete attribute set. Over-limit sets are [`Self::Opaque`].
    AttributeSet(serde_json::Map<String, Value>),
    /// A complete submodule attribute set. Over-limit sets are [`Self::Opaque`].
    Submodule(serde_json::Map<String, Value>),
    /// A function or another value that cannot be serialized safely.
    Opaque { type_name: String },
    /// Evaluation did not produce a value.
    Failed(SafeEvaluationError),
}

/// Describes a package without requiring the package output to remain in store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafePackageValue {
    /// Redacted package display name, when Nix provided one.
    pub name: Option<String>,
    /// Redacted package pname, when Nix provided one.
    pub pname: Option<String>,
    /// Redacted package version, when Nix provided one.
    pub version: Option<String>,
    /// Redacted output path marker, when Nix provided an output path.
    pub output_path: Option<String>,
}

/// Describes a failed or deliberately unsupported option evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafeEvaluationError {
    /// Stable machine-readable failure category.
    pub code: String,
    /// Redaction placeholder that does not retain evaluator-controlled text.
    pub message: String,
}

/// Identifies one option definition and its source provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptionDefinitionProvenance {
    /// Source path reported by the Nix module system.
    pub source_path: String,
    /// Source input name when it can be resolved from tracked flake metadata.
    pub source_input: Option<String>,
    /// Full source revision when it can be resolved.
    pub source_revision: Option<String>,
    /// Definition value after deterministic safe-value sanitization.
    pub value: Option<Value>,
    /// True only when evaluator metadata identifies this definition as winning.
    pub winning: bool,
    /// Module-system priority when the evaluator can determine it.
    #[serde(default)]
    pub priority: Option<i64>,
    /// Stable provenance status such as `winning` or `overridden`.
    #[serde(default)]
    pub status: Option<String>,
    /// Maintainer-facing explanation of why this definition won or lost.
    #[serde(default)]
    pub winner_note: Option<String>,
    /// Server-issued navigation identity. Evaluator output never persists this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracked_flake: Option<TrackedFlakeIdentity>,
}

/// Contains the safe persisted representation of one NixOS option.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluatedOption {
    /// Full option path.
    pub path: String,
    /// Declared NixOS option type.
    pub declared_type: String,
    /// Tagged evaluated value or explicit failure.
    pub value: SafeOptionValue,
    /// Complete definition provenance emitted by the evaluator.
    pub definitions: Vec<OptionDefinitionProvenance>,
    /// True when evaluator metadata proves that lower-priority definitions exist.
    pub overridden: bool,
}

impl EvaluatedOption {
    /// Returns a conservatively redacted option suitable for persistence.
    ///
    /// Sensitive option paths and nested keys redact their values. Ordinary
    /// strings, package metadata, winner notes, and diagnostics remain useful
    /// unless they match a credential, provider-token, JWT, or high-entropy
    /// token pattern. Provenance paths and URLs use the same text policy.
    pub fn redacted(mut self) -> Self {
        let option_path = self.path.clone();
        self.path = redact_text(&self.path);
        self.declared_type = redact_text(&self.declared_type);
        self.value = redact_safe_value(&option_path, self.value);
        for definition in &mut self.definitions {
            // SECURITY: Tracked identities are response-only. Never include a
            // database-derived navigation capability in persisted evaluator data.
            definition.tracked_flake = None;
            definition.source_path = redact_text(&definition.source_path);
            definition.source_input = definition.source_input.take().map(|v| redact_text(&v));
            definition.source_revision = definition.source_revision.take().map(|v| redact_text(&v));
            definition.value = definition
                .value
                .take()
                .map(|value| redact_option_value(&option_path, &value));
            definition.status = definition.status.take().map(|v| redact_text(&v));
            definition.winner_note = definition
                .winner_note
                .take()
                .map(|value| redact_text(&value));
        }
        self
    }

    /// Returns the canonical SHA-256 digest of an already-redacted option.
    ///
    /// The digest excludes the option path because the same payload can be
    /// shared by different configurations and option paths.
    pub fn content_digest(&self) -> [u8; 32] {
        let payload = serde_json::json!({
            "declared_type": self.declared_type,
            "value": self.value,
            "definitions": self.definitions,
            "overridden": self.overridden,
        });
        let bytes = serde_json::to_vec(&payload).unwrap_or_default();
        Sha256::digest(bytes).into()
    }

    /// Returns bounded searchable text built only from redacted fields.
    pub fn search_text(&self) -> String {
        let mut text = format!("{} ", self.declared_type);
        text.push_str(&serde_json::to_string(&self.value).unwrap_or_default());
        for definition in &self.definitions {
            text.push(' ');
            text.push_str(&definition.source_path);
            if let Some(input) = &definition.source_input {
                text.push(' ');
                text.push_str(input);
            }
        }
        text.replace(REDACTED_VALUE, "")
            .chars()
            .take(16_384)
            .collect()
    }
}

fn redact_safe_value(path: &str, value: SafeOptionValue) -> SafeOptionValue {
    match value {
        SafeOptionValue::Scalar(value) => {
            SafeOptionValue::Scalar(redact_option_value(path, &value))
        }
        SafeOptionValue::Package(mut package) => {
            package.name = package
                .name
                .take()
                .map(|value| redact_value_string(path, value));
            package.pname = package
                .pname
                .take()
                .map(|value| redact_value_string(path, value));
            package.version = package
                .version
                .take()
                .map(|value| redact_value_string(path, value));
            package.output_path = package
                .output_path
                .take()
                .map(|value| redact_value_string(path, value));
            SafeOptionValue::Package(package)
        }
        SafeOptionValue::List(values) => SafeOptionValue::List(
            values
                .into_iter()
                .map(|value| redact_safe_value(path, value))
                .collect(),
        ),
        SafeOptionValue::AttributeSet(values) => {
            match redact_option_value(path, &Value::Object(values)) {
                Value::Object(values) => SafeOptionValue::AttributeSet(values),
                _ => unreachable!("redacting an object preserves its JSON kind"),
            }
        }
        SafeOptionValue::Submodule(values) => {
            match redact_option_value(path, &Value::Object(values)) {
                Value::Object(values) => SafeOptionValue::Submodule(values),
                _ => unreachable!("redacting an object preserves its JSON kind"),
            }
        }
        SafeOptionValue::Opaque { type_name } => SafeOptionValue::Opaque {
            type_name: match type_name.as_str() {
                "lambda" | "list_over_limit" | "attribute_set_over_limit" => type_name,
                _ => redact_text(&type_name),
            },
        },
        SafeOptionValue::Failed(mut error) => {
            error.code = match error.code.as_str() {
                "not_evaluated" | "over_limit" | "over_depth" => error.code,
                _ => REDACTED_VALUE.to_string(),
            };
            error.message = match redact_option_value(path, &Value::String(error.message.clone())) {
                Value::String(value) if value == REDACTED_VALUE => value,
                _ => redact_evaluation_error(&error.message),
            };
            SafeOptionValue::Failed(error)
        }
    }
}

fn redact_value_string(path: &str, value: String) -> String {
    redact_option_value(path, &Value::String(value))
        .as_str()
        .unwrap_or(REDACTED_VALUE)
        .to_string()
}

/// Computes a type-aware diff without coercing failed or opaque values.
pub fn typed_option_diff(
    before: Option<&EvaluatedOption>,
    after: Option<&EvaluatedOption>,
) -> TypedOptionDiff {
    let kind = match (before, after) {
        (None, Some(_)) => OptionChangeKind::Added,
        (Some(_), None) => OptionChangeKind::Removed,
        (Some(before), Some(after)) if before == after => OptionChangeKind::Unchanged,
        (Some(_), Some(_)) => OptionChangeKind::Modified,
        (None, None) => OptionChangeKind::Unchanged,
    };
    let value_kind = after
        .or(before)
        .map(|option| safe_value_kind(&option.value))
        .unwrap_or("unknown")
        .to_string();

    let (added, removed) = match (before.map(|v| &v.value), after.map(|v| &v.value)) {
        (Some(SafeOptionValue::List(before)), Some(SafeOptionValue::List(after))) => {
            collection_delta(before, after)
        }
        (
            Some(SafeOptionValue::AttributeSet(before)),
            Some(SafeOptionValue::AttributeSet(after)),
        )
        | (Some(SafeOptionValue::Submodule(before)), Some(SafeOptionValue::Submodule(after))) => {
            attribute_delta(before, after)
        }
        (before, after) => (
            after
                .filter(|after| before != Some(*after))
                .and_then(|value| serde_json::to_value(value).ok())
                .into_iter()
                .collect(),
            before
                .filter(|before| after != Some(*before))
                .and_then(|value| serde_json::to_value(value).ok())
                .into_iter()
                .collect(),
        ),
    };
    TypedOptionDiff {
        kind,
        value_kind,
        added,
        removed,
    }
}

fn safe_value_kind(value: &SafeOptionValue) -> &'static str {
    match value {
        SafeOptionValue::Scalar(_) => "scalar",
        SafeOptionValue::Package(_) => "package",
        SafeOptionValue::List(values)
            if values
                .iter()
                .all(|value| matches!(value, SafeOptionValue::Package(_))) =>
        {
            "package_collection"
        }
        SafeOptionValue::List(_) => "list",
        SafeOptionValue::AttributeSet(_) => "attribute_set",
        SafeOptionValue::Submodule(_) => "submodule",
        SafeOptionValue::Opaque { .. } => "opaque",
        SafeOptionValue::Failed(_) => "failed",
    }
}

fn collection_delta(
    before: &[SafeOptionValue],
    after: &[SafeOptionValue],
) -> (Vec<Value>, Vec<Value>) {
    let encoded_before = before
        .iter()
        .filter_map(|value| serde_json::to_value(value).ok())
        .collect::<Vec<_>>();
    let encoded_after = after
        .iter()
        .filter_map(|value| serde_json::to_value(value).ok())
        .collect::<Vec<_>>();
    let mut remaining_before = encoded_before.clone();
    let mut added = Vec::new();
    for value in encoded_after {
        if let Some(index) = remaining_before
            .iter()
            .position(|candidate| candidate == &value)
        {
            remaining_before.remove(index);
        } else {
            added.push(value);
        }
    }
    (added, remaining_before)
}

fn attribute_delta(
    before: &serde_json::Map<String, Value>,
    after: &serde_json::Map<String, Value>,
) -> (Vec<Value>, Vec<Value>) {
    let added = after
        .iter()
        .filter(|(key, value)| before.get(*key) != Some(*value))
        .map(|(key, value)| serde_json::json!({"key": key, "value": value}))
        .collect();
    let removed = before
        .iter()
        .filter(|(key, value)| after.get(*key) != Some(*value))
        .map(|(key, value)| serde_json::json!({"key": key, "value": value}))
        .collect();
    (added, removed)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn generation_params_do_not_require_a_revision() {
        let params: EvaluatedOptionsParams = serde_json::from_value(serde_json::json!({
            "mode": "generation",
            "generation": 8
        }))
        .expect("generation parameters should deserialize without a commit revision");

        assert_eq!(params.mode, SnapshotRevisionMode::Generation);
        assert_eq!(params.generation, Some(8));
        assert!(params.revision.is_empty());
    }

    #[test]
    fn tracked_flake_identity_is_response_only() {
        let identity = TrackedFlakeIdentity {
            flake_id: 7,
            flake_name: "tracked".into(),
            repo_url: "https://example.test/tracked.git".into(),
            revision: "a".repeat(40),
        };
        let mut definition = OptionDefinitionProvenance {
            source_path: "module.nix".into(),
            source_input: Some("self".into()),
            source_revision: Some("a".repeat(40)),
            value: None,
            winning: true,
            priority: None,
            status: None,
            winner_note: None,
            tracked_flake: Some(identity),
        };
        assert!(
            serde_json::to_value(&definition)
                .expect("response provenance should serialize")
                .get("tracked_flake")
                .is_some()
        );

        definition = EvaluatedOption {
            path: "services.example.enable".into(),
            declared_type: "boolean".into(),
            value: SafeOptionValue::Scalar(json!(true)),
            definitions: vec![definition],
            overridden: false,
        }
        .redacted()
        .definitions
        .remove(0);
        assert!(definition.tracked_flake.is_none());
        assert!(
            serde_json::to_value(definition)
                .expect("persisted provenance should serialize")
                .get("tracked_flake")
                .is_none()
        );
    }

    #[test]
    fn redaction_precedes_digest_and_search_indexing() {
        let option = EvaluatedOption {
            path: "services.example.password".into(),
            declared_type: "string".into(),
            value: SafeOptionValue::Scalar(json!({"token": "secret-value"})),
            definitions: vec![OptionDefinitionProvenance {
                source_path: "https://user:pass@example.test/flake?token=hidden".into(),
                source_input: Some("nixpkgs".into()),
                source_revision: Some("a".repeat(40)),
                value: Some(json!({"api_key": "another-secret"})),
                winning: false,
                priority: Some(1000),
                status: Some("overridden".into()),
                winner_note: Some("higher-priority secret note".into()),
                tracked_flake: None,
            }],
            overridden: false,
        }
        .redacted();

        let serialized = serde_json::to_string(&option).unwrap();
        let search = option.search_text();
        for secret in ["secret-value", "user:pass", "hidden", "another-secret"] {
            assert!(!serialized.contains(secret));
            assert!(!search.contains(secret));
        }
        assert_eq!(option.content_digest().len(), 32);
        assert_eq!(
            option.value,
            SafeOptionValue::Scalar(json!({REDACTED_VALUE: REDACTED_VALUE}))
        );
    }

    #[test]
    fn sensitive_path_redacts_every_safe_value_shape_and_definition_default() {
        let values = [
            SafeOptionValue::Scalar(json!(null)),
            SafeOptionValue::Scalar(json!(false)),
            SafeOptionValue::Scalar(json!(8675309)),
            SafeOptionValue::Scalar(json!("plain-secret")),
            SafeOptionValue::Package(SafePackageValue {
                name: Some("package-secret".into()),
                pname: None,
                version: None,
                output_path: None,
            }),
            SafeOptionValue::List(vec![SafeOptionValue::Scalar(json!("list-secret"))]),
            SafeOptionValue::AttributeSet(
                serde_json::from_value::<serde_json::Map<String, Value>>(
                    json!({"safe": "nested-secret"}),
                )
                .unwrap(),
            ),
            SafeOptionValue::Failed(SafeEvaluationError {
                code: "failed".into(),
                message: "error-secret".into(),
            }),
        ];

        for value in values {
            let option = EvaluatedOption {
                path: "services.example.token".into(),
                declared_type: "anything".into(),
                value,
                definitions: vec![OptionDefinitionProvenance {
                    source_path: "module.nix".into(),
                    source_input: None,
                    source_revision: None,
                    value: Some(json!("default-secret")),
                    winning: true,
                    priority: Some(100),
                    status: Some("winning".into()),
                    winner_note: None,
                    tracked_flake: None,
                }],
                overridden: false,
            }
            .redacted();
            let encoded = serde_json::to_string(&option).unwrap();
            for secret in [
                "plain-secret",
                "package-secret",
                "list-secret",
                "nested-secret",
                "error-secret",
                "default-secret",
            ] {
                assert!(!encoded.contains(secret));
            }
        }
    }

    #[test]
    fn safe_strings_remain_distinct_and_secret_keys_are_not_serialized() {
        let option = EvaluatedOption {
            path: "services.example.aliases".into(),
            declared_type: "attribute set".into(),
            value: SafeOptionValue::AttributeSet(
                serde_json::from_value(json!({
                    "GITHUB_PAT": "github-secret",
                    "displayName": "production package set",
                    "enabled": true,
                    "retries": 2
                }))
                .unwrap(),
            ),
            definitions: vec![OptionDefinitionProvenance {
                source_path: "https://example.test/module.nix".into(),
                source_input: Some("self".into()),
                source_revision: Some("a".repeat(40)),
                value: Some(json!({"kind": "scalar", "value": "documented default"})),
                winning: true,
                priority: Some(100),
                status: Some("winning".into()),
                winner_note: Some("A lower numeric module-system priority won.".into()),
                tracked_flake: None,
            }],
            overridden: false,
        }
        .redacted();

        let encoded = serde_json::to_string(&option).unwrap();
        let search = option.search_text();
        for secret in ["GITHUB_PAT", "github-secret"] {
            assert!(!encoded.contains(secret));
            assert!(!search.contains(secret));
        }
        assert!(encoded.contains("production package set"));
        assert!(encoded.contains("documented default"));
        assert!(encoded.contains("lower numeric module-system priority"));
        assert!(search.contains("production package set"));
        assert!(encoded.contains("true"));
        assert!(encoded.contains('2'));
    }

    #[test]
    fn safe_package_values_produce_distinct_diffs_and_search_text() {
        let package = |name: &str, version: &str| {
            EvaluatedOption {
                path: "environment.systemPackages".into(),
                declared_type: "list of package".into(),
                value: SafeOptionValue::List(vec![SafeOptionValue::Package(SafePackageValue {
                    name: Some(format!("{name}-{version}")),
                    pname: Some(name.into()),
                    version: Some(version.into()),
                    output_path: Some(format!(
                        "/nix/store/0123456789abcdfghijklmnpqrsvwxyz-{name}-{version}"
                    )),
                })]),
                definitions: Vec::new(),
                overridden: false,
            }
            .redacted()
        };
        let before = package("curl", "8.9.0");
        let after = package("curl", "8.10.0");

        assert_ne!(before.content_digest(), after.content_digest());
        assert!(before.search_text().contains("curl-8.9.0"));
        assert!(after.search_text().contains("curl-8.10.0"));
        let diff = typed_option_diff(Some(&before), Some(&after));
        assert_eq!(diff.kind, OptionChangeKind::Modified);
        assert_ne!(diff.added, diff.removed);
    }

    #[test]
    fn identical_redacted_payloads_deduplicate_across_paths() {
        let make = |path: &str| EvaluatedOption {
            path: path.into(),
            declared_type: "boolean".into(),
            value: SafeOptionValue::Scalar(json!(true)),
            definitions: Vec::new(),
            overridden: false,
        };

        assert_eq!(make("a").content_digest(), make("b").content_digest());
    }

    #[test]
    fn failed_and_opaque_values_remain_explicit() {
        let failed = SafeOptionValue::Failed(SafeEvaluationError {
            code: "not_evaluated".into(),
            message: "Option value did not evaluate".into(),
        });
        let opaque = SafeOptionValue::Opaque {
            type_name: "lambda".into(),
        };

        assert_eq!(serde_json::to_value(failed).unwrap()["kind"], "failed");
        assert_eq!(serde_json::to_value(opaque).unwrap()["kind"], "opaque");
    }

    #[test]
    fn package_collection_diff_preserves_additions_and_removals() {
        let option = |packages: &[&str]| EvaluatedOption {
            path: "environment.systemPackages".into(),
            declared_type: "list of package".into(),
            value: SafeOptionValue::List(
                packages
                    .iter()
                    .map(|name| {
                        SafeOptionValue::Package(SafePackageValue {
                            name: Some((*name).into()),
                            pname: Some((*name).into()),
                            version: None,
                            output_path: None,
                        })
                    })
                    .collect(),
            ),
            definitions: Vec::new(),
            overridden: false,
        };
        let before = option(&["curl", "git"]);
        let after = option(&["git", "jq"]);
        let diff = typed_option_diff(Some(&before), Some(&after));

        assert_eq!(diff.kind, OptionChangeKind::Modified);
        assert_eq!(diff.value_kind, "package_collection");
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.removed.len(), 1);
        assert!(diff.added[0].to_string().contains("jq"));
        assert!(diff.removed[0].to_string().contains("curl"));
    }

    #[test]
    fn removed_option_is_explicit() {
        let before = EvaluatedOption {
            path: "services.old.enable".into(),
            declared_type: "boolean".into(),
            value: SafeOptionValue::Scalar(json!(true)),
            definitions: Vec::new(),
            overridden: false,
        };
        let diff = typed_option_diff(Some(&before), None);
        assert_eq!(diff.kind, OptionChangeKind::Removed);
        assert!(diff.added.is_empty());
        assert_eq!(diff.removed.len(), 1);
    }

    #[test]
    fn flake_delta_reports_system_module_and_resolved_input_changes() {
        let before = json!({
            "declared_systems": ["alpha", "old"],
            "exported_modules": [{"name": "base"}, {"name": "old-module"}],
            "inputs": [
                {"node": "nixpkgs", "locked_revision": "a".repeat(40)},
                {"node": "removed", "locked_revision": "c".repeat(40)}
            ]
        });
        let after = json!({
            "declared_systems": ["alpha", "beta"],
            "exported_modules": [{"name": "base"}, {"name": "new-module"}],
            "inputs": [
                {"node": "nixpkgs", "locked_revision": "b".repeat(40)},
                {"node": "added", "locked_revision": "d".repeat(40)}
            ]
        });

        let delta = flake_output_delta(&before, &after);
        assert_eq!(delta.systems_added, ["beta"]);
        assert_eq!(delta.systems_removed, ["old"]);
        assert_eq!(delta.modules_added, ["new-module"]);
        assert_eq!(delta.modules_removed, ["old-module"]);
        assert_eq!(delta.inputs_added, ["added"]);
        assert_eq!(delta.inputs_removed, ["removed"]);
        assert_eq!(delta.input_revision_bumps.len(), 1);
        assert_eq!(delta.systems_added_total, 1);
        assert_eq!(delta.systems_removed_total, 1);
        assert_eq!(delta.modules_added_total, 1);
        assert_eq!(delta.modules_removed_total, 1);
        assert_eq!(delta.inputs_added_total, 1);
        assert_eq!(delta.inputs_removed_total, 1);
        assert_eq!(delta.input_revision_bumps_total, 1);
    }
}
