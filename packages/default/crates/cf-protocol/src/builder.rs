//! Builder ↔ server wire protocol types.
//!
//! These types are serialized over HTTP between the Crystal Forge server and
//! remote build workers. No database, Axum, or server-internal types are
//! permitted here.

use crate::cache::CacheType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Maximum encoded CVE completion request size accepted by the protocol.
pub const CVE_SCAN_MAX_BODY_BYTES: u64 = 8 * 1024 * 1024;
/// Maximum package evidence entries accepted in one CVE result.
pub const CVE_SCAN_MAX_ENTRIES: usize = 50_000;
/// Maximum package-to-CVE observations accepted in one CVE result.
pub const CVE_SCAN_MAX_OBSERVATIONS: usize = 250_000;
/// Current structured CVE result schema advertised by capable builders.
pub const CVE_SCAN_SCHEMA_VERSION: u32 = 1;
/// Current evaluator contract understood by verified-source builders.
pub const VERIFIED_SOURCE_EVALUATOR_CONTRACT_VERSION: u32 = 1;
/// Current canonical Git-tree-to-Nix-store materialization schema.
pub const VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION: u32 = 1;

/// Returns whether `value` is one canonical direct child of `/nix/store`.
///
/// Canonical paths contain exactly one non-empty basename after
/// `/nix/store/`. They do not contain traversal components, repeated
/// separators, or a trailing separator. When `derivation` is true, the
/// basename must end in `.drv`.
pub fn is_canonical_nix_store_path(value: &str, derivation: bool) -> bool {
    let Some(basename) = value.strip_prefix("/nix/store/") else {
        return false;
    };
    !basename.is_empty()
        && basename != "."
        && basename != ".."
        && !basename.contains(['/', '\\', '\0'])
        && (!derivation || basename.ends_with(".drv"))
}

/// Describes optional work that a builder can execute.
///
/// Missing capabilities deserialize to version `0`, which means incapable.
/// This default lets old builder JSON remain valid during rolling upgrades.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BuilderCapabilities {
    /// Whether this builder process accepts CVE scan leases.
    pub cve_scanning: bool,
    /// Structured CVE result schema supported by the builder, or `0` when CVE
    /// scanning is disabled or unsupported.
    pub cve_scan_schema_version: u32,
    /// Scanner implementation advertised by this builder process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cve_scanner: Option<CveScannerIdentity>,
}

impl BuilderCapabilities {
    /// Returns capabilities for a builder that supports the current CVE schema.
    pub fn current_cve_scanner(version: String) -> Self {
        Self {
            cve_scanning: true,
            cve_scan_schema_version: CVE_SCAN_SCHEMA_VERSION,
            cve_scanner: Some(CveScannerIdentity {
                name: "vulnix".to_string(),
                version,
            }),
        }
    }

    /// Returns whether the builder supports the current CVE schema.
    pub fn supports_current_cve_schema(&self) -> bool {
        self.cve_scanning
            && self.cve_scan_schema_version == CVE_SCAN_SCHEMA_VERSION
            && self.cve_scanner.as_ref().is_some_and(|scanner| {
                scanner.name == "vulnix"
                    && !scanner.version.trim().is_empty()
                    && scanner.version.chars().count() <= 50
            })
    }
}

// =============================================================================
// EXECUTION STRATEGY TYPES
// =============================================================================

/// Explicit remote build execution strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteBuildExecutionStrategy {
    /// Server evaluates and provides the authoritative `.drv` path.
    #[default]
    ServerDerivation,
    /// Builder evaluates immutable source locally and must match the server's
    /// expected `.drvPath` before building.
    SourceReEvaluateVerified,
}

/// Source/input delivery mode for verified source re-evaluation.
///
/// Evaluator contract version 1 accepts only [`Self::ServerBundledArchive`].
/// The server rejects every other mode before claim and does not fall back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceInputDeliveryMode {
    /// Not applicable for the current job strategy.
    #[default]
    None,
    /// Server serves the canonical tracked-tree tar artifact used by its
    /// authoritative evaluator through an authenticated API endpoint. The
    /// builder verifies its authorized size and SHA-256 digest before extraction.
    ///
    /// **Scope:** only the top-level repository is bundled. Locked flake
    /// inputs that are NOT already in the builder's Nix store or reachable
    /// via configured substituters may still require network access during
    /// `nix eval`. Private flake inputs must be publicly accessible, cached,
    /// or pre-seeded on the builder for air-gapped operation.
    ServerBundledArchive,
    /// Reserved local Git worktree delivery mode.
    ///
    /// Evaluator contract version 1 does not support this mode.
    LocalGitWorktree,
    /// Reserved builder-side public input fetch mode.
    ///
    /// Evaluator contract version 1 does not support this mode.
    BuilderFetchPublicInputs,
}

// =============================================================================
// SOURCE IDENTITY
// =============================================================================

/// Immutable source identity for verified source re-evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedSourceIdentity {
    /// Credential-free repository locator used to identify the bare mirror.
    pub repo_url: String,
    /// Full Git commit object ID authorized by the server.
    pub commit_hash: String,
    /// Flake output attribute evaluated from the immutable source.
    pub flake_target: String,
    /// Stable server-selected mirror identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror_id: Option<String>,
    /// Legacy builder-local mirror path; absent for contract version 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror_path: Option<String>,
    /// Legacy builder-local worktree path; absent for contract version 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    /// Legacy lock-file digest retained for wire compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_hash: Option<String>,
    /// Authenticated server route for the canonical source artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_url: Option<String>,
    /// Legacy artifact digest mirrored from [`Self::immutable_source`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_sha256: Option<String>,
    /// Canonical Nix store source authorized during server evaluation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub immutable_source: Option<ImmutableSourceIdentity>,
}

/// Identifies a canonical tracked Git tree after Nix store ingestion.
///
/// The NAR hash and store name form the portable identity. A builder MUST
/// recreate and verify both values from the server-bundled commit before
/// evaluation. The server store path is an audit value and MUST NOT be trusted
/// as a builder-local path because stores can use different roots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImmutableSourceIdentity {
    /// Materialization algorithm version.
    pub schema_version: u32,
    /// Name passed to `nix store add-path` on every evaluator host.
    pub store_name: String,
    /// SRI SHA-256 hash of the canonical source NAR.
    pub nar_hash: String,
    /// SHA-256 hex digest of the committed `flake.lock` bytes.
    pub lock_hash: String,
    /// Canonical source artifact format version.
    pub artifact_format_version: u32,
    /// SHA-256 hex digest of the exact artifact bytes consumed by both hosts.
    pub artifact_sha256: String,
    /// Exact artifact size in bytes.
    pub artifact_size: u64,
    /// Store path produced on the authoritative server, for audit diagnostics.
    pub server_store_path: String,
}

/// Records the evaluator dimensions enforced before verified re-evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluatorFingerprint {
    /// Evaluator contract schema, or `0` for a legacy fingerprint.
    #[serde(default)]
    pub contract_version: u32,
    /// Nix language version reported by the evaluator that executes the job.
    pub nix_version: String,
    /// Nix system reported by `builtins.currentSystem` for the evaluator.
    #[serde(default)]
    pub evaluator_system: String,
    /// Whether evaluation prohibits access to ambient host state.
    #[serde(default)]
    pub pure_eval: bool,
    /// Whether evaluation can update or create `flake.lock`.
    #[serde(default)]
    pub lockfile_mutation_allowed: bool,
    /// Explicit `allow-import-from-derivation` evaluator setting.
    #[serde(default)]
    pub allow_import_from_derivation: bool,
    /// Canonical source materialization schema used by the evaluator.
    #[serde(default)]
    pub source_materialization_schema_version: u32,
}

// =============================================================================
// CACHE PUSH CONFIG
// =============================================================================

/// Cache-push settings selected by the server for a remote builder job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderCachePushConfig {
    #[serde(default)]
    pub cache_type: CacheType,
    pub push_to: Option<String>,
    #[serde(default)]
    pub push_after_build: bool,
    pub signing_key: Option<String>,
    pub compression: Option<String>,
    pub s3_region: Option<String>,
    pub s3_profile: Option<String>,
    pub s3_access_key_id: Option<String>,
    pub s3_secret_access_key: Option<String>,
    pub s3_session_token: Option<String>,
    pub s3_endpoint_url: Option<String>,
    pub attic_token: Option<String>,
    pub attic_cache_name: Option<String>,
    pub attic_public_key: Option<String>,
    #[serde(default)]
    pub attic_ignore_upstream_cache_filter: bool,
    #[serde(default)]
    pub attic_jobs: u32,
    #[serde(default)]
    pub max_retries: u32,
    #[serde(default)]
    pub retry_delay_seconds: u64,
    #[serde(default = "default_push_timeout_seconds")]
    pub push_timeout_seconds: u64,
    #[serde(default)]
    pub force_repush: bool,
    #[serde(default)]
    pub require_sigs: bool,
}

fn default_push_timeout_seconds() -> u64 {
    3600 // 1 hour
}

impl BuilderCachePushConfig {
    pub fn disabled() -> Self {
        Self {
            cache_type: CacheType::Nix,
            push_to: None,
            push_after_build: false,
            signing_key: None,
            compression: None,
            s3_region: None,
            s3_profile: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_session_token: None,
            s3_endpoint_url: None,
            attic_token: None,
            attic_cache_name: None,
            attic_public_key: None,
            attic_ignore_upstream_cache_filter: true,
            attic_jobs: 5,
            max_retries: 3,
            retry_delay_seconds: 5,
            push_timeout_seconds: default_push_timeout_seconds(),
            force_repush: false,
            require_sigs: true,
        }
    }
}

// =============================================================================
// BUILD JOB WIRE TYPES
// =============================================================================

/// Minimal derivation build payload delivered to API-mode builders so they can
/// build without any direct database access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildJobDerivation {
    pub id: i32,
    pub derivation_name: String,
    /// "nixos" or "package"
    pub derivation_type: String,
    /// .drv path populated during the dry-run/eval phase.
    pub derivation_path: Option<String>,
    /// Resolved output store path, if already known.
    pub store_path: Option<String>,
    /// Explicit remote build execution strategy.
    #[serde(default)]
    pub execution_strategy: RemoteBuildExecutionStrategy,
    /// Source metadata used by verified source re-evaluation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<VerifiedSourceIdentity>,
    /// How the builder should obtain flake inputs for local evaluation.
    #[serde(default)]
    pub source_input_delivery: SourceInputDeliveryMode,
    /// Server-authorized toplevel derivation identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_drv_path: Option<String>,
    /// Server-recorded evaluator fingerprint for audit/debugging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluator: Option<EvaluatorFingerprint>,
    /// Server-selected cache destination for builder-side output pushes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_push: Option<BuilderCachePushConfig>,
}

/// Build job wire representation (as delivered in `NextJobResponse`).
///
/// This is the serde-only form for the builder ↔ server API.
/// The server maps from its DB row type before sending.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildJob {
    pub id: Uuid,
    pub builder_id: Option<Uuid>,
    #[serde(default)]
    pub builder_session_id: Option<Uuid>,
    pub derivation_id: i32,
    pub environment_id: Option<Uuid>,
    pub status: String,
    pub retry_count: i32,
    pub max_retries: i32,
    #[serde(default)]
    pub parent_job_id: Option<Uuid>,
    #[serde(default)]
    pub root_job_id: Option<Uuid>,
    #[serde(default = "default_attempt_number")]
    pub attempt_number: i32,
    #[serde(default = "Utc::now")]
    pub available_at: DateTime<Utc>,
    pub priority_weight: f64,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub logs: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_attempt_number() -> i32 {
    1
}

/// Response returned by GET/POST /api/v1/builders/:id/next-job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextJobResponse {
    pub job: BuildJob,
    pub derivation: BuildJobDerivation,
}

/// Signed request body for POST /api/v1/builders/:id/next-job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextJobRequest {
    /// Builder polling protocol version.
    #[serde(default = "default_builder_protocol_version")]
    pub protocol_version: u32,
    /// Remote execution strategies that this builder can execute.
    #[serde(default = "default_supported_execution_strategies")]
    pub supported_execution_strategies: Vec<RemoteBuildExecutionStrategy>,
    /// Evaluator contracts that this builder can validate before evaluation.
    #[serde(default)]
    pub supported_evaluator_contract_versions: Vec<u32>,
    /// Effective evaluator settings probed before this builder starts polling.
    ///
    /// Legacy requests omit this field and cannot claim verified-source work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluator: Option<EvaluatorFingerprint>,
}

/// Returns the NAR-qualified flake reference for an immutable Nix store path.
///
/// The function percent-encodes the NAR hash as an RFC 3986 query value. The
/// server and builder MUST use the returned reference for contract-v1
/// evaluation so they resolve identical flake inputs.
pub fn nar_qualified_store_flake_ref(store_path: &str, nar_hash: &str) -> String {
    let encoded_hash = nar_hash.bytes().fold(String::new(), |mut output, byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
        output
    });
    format!("path:{store_path}?narHash={encoded_hash}")
}

fn default_builder_protocol_version() -> u32 {
    1
}

fn default_supported_execution_strategies() -> Vec<RemoteBuildExecutionStrategy> {
    vec![RemoteBuildExecutionStrategy::ServerDerivation]
}

// =============================================================================
// BUILDER SESSION / REGISTRATION TYPES
// =============================================================================

/// Request to resolve a builder's server-assigned ID from its public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveBuilderIdRequest {
    /// Base64-encoded Ed25519 public key derived from the builder's local private key.
    pub public_key: String,
    /// Per-process session UUID generated on builder startup.
    #[serde(default)]
    pub session_id: Option<Uuid>,
    /// Optional work supported by this builder process.
    #[serde(default)]
    pub capabilities: BuilderCapabilities,
}

/// Response returned when a builder public key has been registered/approved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveBuilderIdResponse {
    pub builder_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
}

/// Request to establish a process/session for a configured builder ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstablishBuilderSessionRequest {
    pub session_id: Uuid,
    /// Optional work supported by this builder process.
    #[serde(default)]
    pub capabilities: BuilderCapabilities,
}

/// Response returned after establishing a builder process/session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstablishBuilderSessionResponse {
    pub builder_id: Uuid,
    pub session_id: Uuid,
    pub recovered_jobs: usize,
}

// =============================================================================
// BUILD PROGRESS / STATUS REPORTING
// =============================================================================

/// Distinct pre-build/build failure phases reported by API builders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildFailurePhase {
    /// The builder could not obtain or inspect the authorized source.
    SourceFetch,
    /// The source commit, lock file, or canonical NAR identity did not match.
    SourceIdentityMismatch,
    /// A required source input was unavailable to the builder.
    SourceInputAvailability,
    /// The builder could not reproduce the server's evaluator contract.
    EvaluatorIncompatible,
    /// Nix could not evaluate the verified source.
    Evaluation,
    /// The evaluated derivation differed from the server-authorized derivation.
    DerivationMismatch,
    /// The builder could not make the authorized derivation locally available.
    PathMaterialization,
    /// The authorized derivation failed to build.
    Build,
}

/// Retry classification supplied by newer builders. Missing values from older
/// builders remain unknown and are not transient-retry eligible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildFailureClass {
    Transient,
    Deterministic,
    Authorization,
    Cancelled,
    Unknown,
}

impl std::fmt::Display for BuildFailurePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildFailurePhase::SourceFetch => write!(f, "source_fetch"),
            BuildFailurePhase::SourceIdentityMismatch => write!(f, "source_identity_mismatch"),
            BuildFailurePhase::SourceInputAvailability => write!(f, "source_input_availability"),
            BuildFailurePhase::EvaluatorIncompatible => write!(f, "evaluator_incompatible"),
            BuildFailurePhase::Evaluation => write!(f, "evaluation"),
            BuildFailurePhase::DerivationMismatch => write!(f, "derivation_mismatch"),
            BuildFailurePhase::PathMaterialization => write!(f, "path_materialization"),
            BuildFailurePhase::Build => write!(f, "build"),
        }
    }
}

/// Build progress report sent by API builders (HTTP fallback for the WS frame).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildProgressRequest {
    pub derivation_id: i32,
    pub elapsed_seconds: i32,
    pub current_target: Option<String>,
    pub last_activity_seconds: i32,
}

/// Request to report builder metrics (via heartbeat).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReportMetricsRequest {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: i64,
    pub system_cpu_usage_percent: Option<f64>,
    pub system_memory_total_mb: Option<i64>,
    pub system_memory_used_mb: Option<i64>,
    /// Optional work supported by this builder process.
    #[serde(default)]
    pub capabilities: BuilderCapabilities,
}

/// Append build logs request.
#[derive(Debug, Serialize, Deserialize)]
pub struct AppendLogsRequest {
    pub logs: String,
}

// =============================================================================
// DERIVATION MANIFEST / ARCHIVE
// =============================================================================

/// Response for GET /api/v1/builders/:id/jobs/:job_id/derivation-manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivationManifestResponse {
    pub job_id: Uuid,
    pub drv_path: String,
    /// Sorted, deduplicated requisite store paths for `drv_path`.
    pub paths: Vec<String>,
}

/// Request body for POST /api/v1/builders/:id/jobs/:job_id/derivation-archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivationArchiveRequest {
    pub paths: Vec<String>,
}

// =============================================================================
// CACHE PUSH REPORTING
// =============================================================================

/// A cache-push job handed to an API builder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePushJobPayload {
    pub id: Uuid,
    pub derivation_id: i32,
    pub derivation_name: String,
    /// Path to push: store_path or derivation_path.
    pub path: String,
    /// Optional cache destination name for last-used bookkeeping.
    pub cache_destination_name: Option<String>,
}

/// Result of a successful cache push reported by an API builder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePushCompleteRequest {
    pub duration_ms: Option<i32>,
}

/// Failure report for a cache push.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePushFailRequest {
    pub error_message: String,
}

// =============================================================================
// CVE SCAN REPORTING
// =============================================================================

/// Identifies the only structured CVE result schema accepted by this version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub enum CveScanSchemaVersion {
    /// Schema 1 carries exact derivation outputs and structured observations.
    V1,
}

impl TryFrom<u32> for CveScanSchemaVersion {
    type Error = String;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            CVE_SCAN_SCHEMA_VERSION => Ok(Self::V1),
            _ => Err(format!("unsupported CVE scan schema version {value}")),
        }
    }
}

impl From<CveScanSchemaVersion> for u32 {
    fn from(value: CveScanSchemaVersion) -> Self {
        match value {
            CveScanSchemaVersion::V1 => CVE_SCAN_SCHEMA_VERSION,
        }
    }
}

/// Identifies the scanner implementation that produced structured evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CveScannerIdentity {
    /// Stable scanner name, such as `vulnix`.
    pub name: String,
    /// Exact scanner version reported by the builder.
    pub version: String,
}

/// Maps one derivation output name to its exact Nix store path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CveDerivationOutput {
    /// Nix derivation output name, such as `out`.
    pub name: String,
    /// Exact output store path authorized by the server.
    pub store_path: String,
}

/// Identifies the derivation and outputs authorized for one scan execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CveScanDerivation {
    /// Server database identity for the derivation record.
    pub derivation_id: i32,
    /// Human-readable derivation name used for diagnostics.
    pub derivation_name: String,
    /// Exact `.drv` store path that produced the authorized outputs.
    pub drv_path: String,
    /// Exact output-name-to-store-path mappings for the derivation.
    pub outputs: Vec<CveDerivationOutput>,
}

/// Server-selected limits and scanner arguments for one CVE lease.
///
/// The server must not issue values above the protocol maxima. A later server
/// route enforces these limits before evidence is persisted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CveScanPolicy {
    /// Maximum encoded completion body size in bytes.
    #[serde(deserialize_with = "deserialize_cve_max_body_bytes")]
    pub max_body_bytes: u64,
    /// Maximum number of package entries.
    #[serde(deserialize_with = "deserialize_cve_max_entries")]
    pub max_entries: usize,
    /// Maximum number of package-to-CVE observations.
    #[serde(deserialize_with = "deserialize_cve_max_observations")]
    pub max_observations: usize,
    /// Maximum scanner runtime in seconds.
    pub timeout_seconds: u64,
    /// Bounded scanner arguments selected by the server.
    pub scanner_args: Vec<String>,
}

fn deserialize_cve_max_body_bytes<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u64::deserialize(deserializer)?;
    if value == 0 || value > CVE_SCAN_MAX_BODY_BYTES {
        return Err(serde::de::Error::custom(format!(
            "CVE result body limit must be between 1 and {CVE_SCAN_MAX_BODY_BYTES} bytes"
        )));
    }
    Ok(value)
}

fn deserialize_cve_max_entries<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let value = usize::deserialize(deserializer)?;
    if value == 0 || value > CVE_SCAN_MAX_ENTRIES {
        return Err(serde::de::Error::custom(format!(
            "CVE entry limit must be between 1 and {CVE_SCAN_MAX_ENTRIES}"
        )));
    }
    Ok(value)
}

fn deserialize_cve_max_observations<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let value = usize::deserialize(deserializer)?;
    if value == 0 || value > CVE_SCAN_MAX_OBSERVATIONS {
        return Err(serde::de::Error::custom(format!(
            "CVE observation limit must be between 1 and {CVE_SCAN_MAX_OBSERVATIONS}"
        )));
    }
    Ok(value)
}

/// Supplies narrowly scoped cache access for materializing authorized outputs.
///
/// SECURITY: The builder must use this credential only for this lease. It must
/// not log, persist, or forward the credential.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CveScanCacheSource {
    /// Binary cache URL from which authorized outputs can be substituted.
    pub url: String,
    /// Optional Nix cache public key used to verify substituted paths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    /// Optional bearer credential scoped to cache reads for this work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer_token: Option<String>,
}

impl std::fmt::Debug for CveScanCacheSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CveScanCacheSource")
            .field("url", &self.url)
            .field("public_key", &self.public_key)
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

/// Owns a CVE scan lease and fences stale builder processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CveScanLease {
    /// Stable scan identity shared by retries and executions.
    pub scan_id: Uuid,
    /// Unique identity for this lease attempt.
    pub execution_id: Uuid,
    /// Builder that owns this execution.
    pub builder_id: Uuid,
    /// Builder process session that owns this execution.
    pub builder_session_id: Uuid,
}

/// Work returned to a scanner-capable API builder.
///
/// SECURITY: Every field is server-issued but remains untrusted at the server
/// boundary when returned. Later route code must authenticate the builder,
/// enforce lease/session ownership, compare exact derivation identity, validate
/// all bounds, and canonicalize evidence before persistence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CveScanClaim {
    /// Lease identity and ownership fence.
    pub lease: CveScanLease,
    /// Exact derivation and output identity authorized for scanning.
    pub derivation: CveScanDerivation,
    /// Structured result schema required for completion.
    pub schema_version: CveScanSchemaVersion,
    /// Scanner implementation required by server policy.
    pub scanner: CveScannerIdentity,
    /// Server-issued bounded execution policy.
    pub policy: CveScanPolicy,
    /// Optional cache source for materializing missing authorized outputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_source: Option<CveScanCacheSource>,
    /// Time at which the current lease expires without a heartbeat.
    pub lease_expires_at: DateTime<Utc>,
}

/// Requests one CVE scan lease without granting the builder database access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CveScanClaimRequest {
    /// Current builder process session.
    pub builder_session_id: Uuid,
    /// Capabilities used by the server to select compatible work.
    #[serde(default)]
    pub capabilities: BuilderCapabilities,
    /// Successful build job whose outputs remain local on this builder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_build_job_id: Option<Uuid>,
}

/// Returns an optional CVE lease to an API builder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CveScanClaimResponse {
    /// Claimed work, or `None` when build work has priority or no scan is ready.
    pub claim: Option<CveScanClaim>,
}

/// Renews an owned CVE scan execution lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CveScanHeartbeatRequest {
    /// Lease identity and builder-session ownership fence.
    pub lease: CveScanLease,
    /// Package entries collected so far.
    pub entries_collected: usize,
    /// Package-to-CVE observations collected so far.
    pub observations_collected: usize,
}

/// Reports the server decision for a CVE lease heartbeat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CveScanHeartbeatResponse {
    /// Updated lease expiration when the lease remains active.
    pub lease_expires_at: DateTime<Utc>,
    /// Whether the builder must stop work and acknowledge revocation.
    pub revocation_requested: bool,
}

/// Records one package discovered in the exact derivation closure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CvePackageEvidence {
    /// Stable zero-based identifier referenced by observations.
    pub entry_id: u32,
    /// Package name reported by the scanner.
    pub package_name: String,
    /// Package version reported by the scanner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_version: Option<String>,
    /// Exact package `.drv` path.
    pub drv_path: String,
    /// Exact outputs produced by the package derivation.
    pub outputs: Vec<CveDerivationOutput>,
}

/// Records one CVE observation for one package evidence entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CveObservation {
    /// `entry_id` of the package affected by this observation.
    pub entry_id: u32,
    /// Canonical vulnerability identifier reported by the scanner.
    pub cve_id: String,
    /// Optional CVSS score reported by the scanner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cvss_score: Option<f32>,
    /// Optional severity label reported by the scanner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    /// Optional fixed package version reported by the scanner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_version: Option<String>,
    /// Whether vulnix reported the package as affected by this CVE.
    ///
    /// Missing values from existing schema-1 builders mean `true`.
    #[serde(
        default = "cve_observation_affected",
        skip_serializing_if = "cve_observation_is_affected"
    )]
    pub affected: bool,
    /// Whether vulnix classified this observation as allowed by its whitelist.
    ///
    /// Missing values from schema-1 builders mean `false`. This additive marker
    /// preserves whitelist evidence without changing the CVE identity.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub whitelisted: bool,
}

fn cve_observation_affected() -> bool {
    true
}

fn cve_observation_is_affected(value: &bool) -> bool {
    *value
}

/// Contains bounded structured evidence produced by one scanner execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CveScanResult {
    /// Result schema. Unknown, malformed, and legacy versions are rejected.
    pub schema_version: CveScanSchemaVersion,
    /// Exact scanner implementation that produced the result.
    pub scanner: CveScannerIdentity,
    /// Exact top-level derivation identity observed by the builder.
    pub derivation: CveScanDerivation,
    /// Package evidence, bounded to [`CVE_SCAN_MAX_ENTRIES`].
    #[serde(deserialize_with = "deserialize_cve_entries")]
    pub entries: Vec<CvePackageEvidence>,
    /// Package-to-CVE observations, bounded to [`CVE_SCAN_MAX_OBSERVATIONS`].
    #[serde(deserialize_with = "deserialize_cve_observations")]
    pub observations: Vec<CveObservation>,
}

/// Serializes canonical schema-1 CVE evidence to the wire bytes hashed by both
/// builders and the server.
///
/// Callers MUST canonicalize entry, output, observation, and CVE ordering before
/// calling this function. The function intentionally does not reorder evidence,
/// because the server must reject a builder digest that was computed over a
/// different semantic representation.
///
/// # Errors
///
/// Returns an error if JSON serialization fails.
pub fn canonical_cve_result_bytes(result: &CveScanResult) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(result)
}

/// Returns the lowercase SHA-256 digest of canonical schema-1 CVE result bytes.
///
/// # Errors
///
/// Returns an error if canonical JSON serialization fails.
pub fn canonical_cve_result_digest(result: &CveScanResult) -> serde_json::Result<String> {
    let bytes = canonical_cve_result_bytes(result)?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn deserialize_cve_entries<'de, D>(deserializer: D) -> Result<Vec<CvePackageEvidence>, D::Error>
where
    D: Deserializer<'de>,
{
    let values = Vec::deserialize(deserializer)?;
    if values.len() > CVE_SCAN_MAX_ENTRIES {
        return Err(serde::de::Error::custom(format!(
            "CVE result has more than {CVE_SCAN_MAX_ENTRIES} entries"
        )));
    }
    Ok(values)
}

fn deserialize_cve_observations<'de, D>(deserializer: D) -> Result<Vec<CveObservation>, D::Error>
where
    D: Deserializer<'de>,
{
    let values = Vec::deserialize(deserializer)?;
    if values.len() > CVE_SCAN_MAX_OBSERVATIONS {
        return Err(serde::de::Error::custom(format!(
            "CVE result has more than {CVE_SCAN_MAX_OBSERVATIONS} observations"
        )));
    }
    Ok(values)
}

/// Completes an owned CVE scan execution with deterministic evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CveScanCompleteRequest {
    /// Lease identity and builder-session ownership fence.
    pub lease: CveScanLease,
    /// Structured evidence produced by the scanner.
    pub result: CveScanResult,
    /// Lowercase SHA-256 of the builder's deterministic result encoding.
    ///
    /// Builders must sort entries by `(drv_path, entry_id)`, outputs by
    /// `(name, store_path)`, and observations by `(entry_id, cve_id)` before
    /// serializing `result`. A later server commit canonicalizes and verifies
    /// this digest before persistence. Repeated completion with the same digest
    /// is an idempotent retry; a different digest is a conflict.
    pub result_digest_sha256: String,
    /// Scanner wall-clock duration in milliseconds.
    pub scan_duration_ms: u64,
}

/// Reports whether a CVE completion was accepted or already recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CveScanCompleteResponse {
    /// Digest sealed for this scan when completion succeeds.
    pub result_digest_sha256: String,
    /// Whether the same digest was already sealed by an earlier retry.
    pub already_completed: bool,
}

/// Classifies why a builder could not complete a CVE scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CveScanFailureClass {
    /// The scanner or cache source failed in a way that can be retried.
    Transient,
    /// The same authorized inputs will deterministically fail again.
    Deterministic,
    /// The builder could not use server-issued authorization.
    Authorization,
    /// The server revoked the lease or the builder stopped it.
    Cancelled,
}

/// Fails an owned CVE scan execution without changing build outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CveScanFailRequest {
    /// Lease identity and builder-session ownership fence.
    pub lease: CveScanLease,
    /// Retry classification used by server scheduling policy.
    pub failure_class: CveScanFailureClass,
    /// Bounded diagnostic text that must not contain credentials.
    pub error_message: String,
}

/// Confirms the server outcome for a CVE scan failure report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CveScanFailResponse {
    /// Whether this execution was requeued for another scanner or fallback.
    pub requeued: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_job_derivation_defaults_to_server_derivation_strategy() {
        let json = r#"{
            "id": 42,
            "derivation_name": "host-a",
            "derivation_type": "nixos",
            "derivation_path": "/nix/store/server-host-a.drv",
            "store_path": null
        }"#;

        let payload: BuildJobDerivation = serde_json::from_str(json).expect("payload should parse");
        assert_eq!(
            payload.execution_strategy,
            RemoteBuildExecutionStrategy::ServerDerivation
        );
        assert_eq!(payload.source_input_delivery, SourceInputDeliveryMode::None);
        assert_eq!(payload.expected_drv_path, None);
        assert!(payload.cache_push.is_none());
    }

    #[test]
    fn legacy_evaluator_fingerprint_defaults_to_incompatible_contract() {
        let fingerprint: EvaluatorFingerprint = serde_json::from_str(
            r#"{"nix_version":"2.34.5","pure_eval":true,"lockfile_mutation_allowed":false}"#,
        )
        .expect("legacy fingerprint should parse");

        assert_eq!(fingerprint.contract_version, 0);
        assert!(fingerprint.evaluator_system.is_empty());
        assert!(!fingerprint.allow_import_from_derivation);
        assert_eq!(fingerprint.source_materialization_schema_version, 0);
    }

    #[test]
    fn legacy_next_job_request_has_no_evaluator_contract_capability() {
        let request: NextJobRequest = serde_json::from_str(
            r#"{"protocol_version":2,"supported_execution_strategies":["source_re_evaluate_verified"]}"#,
        )
        .expect("legacy request should parse");

        assert!(request.supported_evaluator_contract_versions.is_empty());
        assert!(request.evaluator.is_none());
    }

    #[test]
    fn nar_qualified_store_reference_uses_rfc3986_query_encoding() {
        assert_eq!(
            nar_qualified_store_flake_ref("/nix/store/abc-source", "sha256-a+b/c=d_~",),
            "path:/nix/store/abc-source?narHash=sha256-a%2Bb%2Fc%3Dd_~"
        );
    }

    #[test]
    fn verified_source_strategy_serializes_as_snake_case() {
        let payload = BuildJobDerivation {
            id: 42,
            derivation_name: "host-a".to_string(),
            derivation_type: "nixos".to_string(),
            derivation_path: None,
            store_path: None,
            execution_strategy: RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
            source: Some(VerifiedSourceIdentity {
                repo_url: "https://gitlab.com/example/private.git".to_string(),
                commit_hash: "abc123".to_string(),
                flake_target: "nixosConfigurations.host-a.config.system.build.toplevel".to_string(),
                mirror_id: Some("repo-test".to_string()),
                mirror_path: Some("/var/lib/crystal-forge/flake-mirrors/repo-test.git".to_string()),
                worktree_path: Some(
                    "/var/lib/crystal-forge/flake-worktrees/repo-test/abc123".to_string(),
                ),
                lock_hash: Some("sha256-lock".to_string()),
                archive_url: Some("file:///tmp/source".to_string()),
                archive_sha256: Some("sha256-source".to_string()),
                immutable_source: Some(ImmutableSourceIdentity {
                    schema_version: VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION,
                    store_name: "crystal-forge-source-abc123".to_string(),
                    nar_hash: "sha256-source-nar".to_string(),
                    lock_hash: "lock-sha256".to_string(),
                    artifact_format_version:
                        crate::source_artifact::VERIFIED_SOURCE_ARTIFACT_FORMAT_VERSION,
                    artifact_sha256: "artifact-sha256".to_string(),
                    artifact_size: 1024,
                    server_store_path: "/nix/store/source".to_string(),
                }),
            }),
            source_input_delivery: SourceInputDeliveryMode::ServerBundledArchive,
            expected_drv_path: Some("/nix/store/server-host-a.drv".to_string()),
            evaluator: Some(EvaluatorFingerprint {
                contract_version: VERIFIED_SOURCE_EVALUATOR_CONTRACT_VERSION,
                nix_version: "2.28.0".to_string(),
                evaluator_system: "x86_64-linux".to_string(),
                pure_eval: true,
                lockfile_mutation_allowed: false,
                allow_import_from_derivation: true,
                source_materialization_schema_version:
                    VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION,
            }),
            cache_push: None,
        };

        let value = serde_json::to_value(payload).expect("payload should serialize");
        assert_eq!(value["execution_strategy"], "source_re_evaluate_verified");
        assert_eq!(value["source_input_delivery"], "server_bundled_archive");
        assert_eq!(value["expected_drv_path"], "/nix/store/server-host-a.drv");
    }

    #[test]
    fn build_failure_phase_display() {
        assert_eq!(BuildFailurePhase::SourceFetch.to_string(), "source_fetch");
        assert_eq!(
            BuildFailurePhase::DerivationMismatch.to_string(),
            "derivation_mismatch"
        );
    }

    #[test]
    fn old_failure_payload_has_no_classification() {
        #[derive(Deserialize)]
        struct Failure {
            #[serde(default)]
            failure_class: Option<BuildFailureClass>,
        }

        let failure: Failure = serde_json::from_str(r#"{"error_message":"failed"}"#)
            .expect("older payload should parse");
        assert_eq!(failure.failure_class, None);
    }

    #[test]
    fn old_builder_wire_payloads_default_to_cve_incapable() {
        let resolve: ResolveBuilderIdRequest =
            serde_json::from_str(r#"{"public_key":"key","session_id":null}"#)
                .expect("old registration should parse");
        let session: EstablishBuilderSessionRequest =
            serde_json::from_str(r#"{"session_id":"40000000-0000-0000-0000-000000000004"}"#)
                .expect("old session request should parse");
        let heartbeat: ReportMetricsRequest = serde_json::from_str(
            r#"{
                "cpu_usage_percent":0.0,
                "memory_usage_mb":0,
                "system_cpu_usage_percent":null,
                "system_memory_total_mb":null,
                "system_memory_used_mb":null
            }"#,
        )
        .expect("old heartbeat should parse");

        assert_eq!(resolve.capabilities, BuilderCapabilities::default());
        assert!(!resolve.capabilities.cve_scanning);
        assert!(!resolve.capabilities.supports_current_cve_schema());
        assert_eq!(session.capabilities, BuilderCapabilities::default());
        assert_eq!(heartbeat.capabilities, BuilderCapabilities::default());
        assert!(!heartbeat.capabilities.cve_scanning);
        assert!(!heartbeat.capabilities.supports_current_cve_schema());
    }

    #[test]
    fn current_builder_parses_legacy_registration_responses() {
        let resolve: ResolveBuilderIdResponse = serde_json::from_str(
            r#"{
                "builder_id":"30000000-0000-0000-0000-000000000003",
                "session_id":"40000000-0000-0000-0000-000000000004"
            }"#,
        )
        .expect("old resolve response should parse");
        let established: EstablishBuilderSessionResponse = serde_json::from_str(
            r#"{
                "builder_id":"30000000-0000-0000-0000-000000000003",
                "session_id":"40000000-0000-0000-0000-000000000004",
                "recovered_jobs":0
            }"#,
        )
        .expect("old session response should parse");

        assert_eq!(resolve.builder_id, established.builder_id);
        assert_eq!(resolve.session_id, Some(established.session_id));
    }

    #[test]
    fn cve_claim_roundtrip_preserves_uuid_identity_outputs_and_cache_token() {
        let claim = CveScanClaim {
            lease: CveScanLease {
                scan_id: Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap(),
                execution_id: Uuid::parse_str("20000000-0000-0000-0000-000000000002").unwrap(),
                builder_id: Uuid::parse_str("30000000-0000-0000-0000-000000000003").unwrap(),
                builder_session_id: Uuid::parse_str("40000000-0000-0000-0000-000000000004")
                    .unwrap(),
            },
            derivation: CveScanDerivation {
                derivation_id: 42,
                derivation_name: "host-a".to_string(),
                drv_path: "/nix/store/host-a.drv".to_string(),
                outputs: vec![CveDerivationOutput {
                    name: "out".to_string(),
                    store_path: "/nix/store/host-a".to_string(),
                }],
            },
            schema_version: CveScanSchemaVersion::V1,
            scanner: CveScannerIdentity {
                name: "vulnix".to_string(),
                version: "1.12.0".to_string(),
            },
            policy: CveScanPolicy {
                max_body_bytes: CVE_SCAN_MAX_BODY_BYTES,
                max_entries: CVE_SCAN_MAX_ENTRIES,
                max_observations: CVE_SCAN_MAX_OBSERVATIONS,
                timeout_seconds: 300,
                scanner_args: vec!["--json".to_string()],
            },
            cache_source: Some(CveScanCacheSource {
                url: "https://cache.example.test".to_string(),
                public_key: Some("cache.example.test:key".to_string()),
                bearer_token: Some("lease-token".to_string()),
            }),
            lease_expires_at: Utc::now(),
        };

        let json = serde_json::to_vec(&claim).expect("claim should serialize");
        let decoded: CveScanClaim =
            serde_json::from_slice(&json).expect("claim should deserialize");

        assert_eq!(decoded, claim);
        assert!(!format!("{claim:?}").contains("lease-token"));
    }

    #[test]
    fn cve_completion_roundtrip_preserves_uuid_result_identity() {
        let lease = CveScanLease {
            scan_id: Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap(),
            execution_id: Uuid::parse_str("20000000-0000-0000-0000-000000000002").unwrap(),
            builder_id: Uuid::parse_str("30000000-0000-0000-0000-000000000003").unwrap(),
            builder_session_id: Uuid::parse_str("40000000-0000-0000-0000-000000000004").unwrap(),
        };
        let derivation = CveScanDerivation {
            derivation_id: 42,
            derivation_name: "host-a".to_string(),
            drv_path: "/nix/store/host-a.drv".to_string(),
            outputs: vec![CveDerivationOutput {
                name: "out".to_string(),
                store_path: "/nix/store/host-a".to_string(),
            }],
        };
        let request = CveScanCompleteRequest {
            lease,
            result: CveScanResult {
                schema_version: CveScanSchemaVersion::V1,
                scanner: CveScannerIdentity {
                    name: "vulnix".to_string(),
                    version: "1.12.0".to_string(),
                },
                derivation,
                entries: vec![CvePackageEvidence {
                    entry_id: 0,
                    package_name: "openssl".to_string(),
                    package_version: Some("3.0.0".to_string()),
                    drv_path: "/nix/store/openssl.drv".to_string(),
                    outputs: vec![CveDerivationOutput {
                        name: "out".to_string(),
                        store_path: "/nix/store/openssl".to_string(),
                    }],
                }],
                observations: vec![CveObservation {
                    entry_id: 0,
                    cve_id: "CVE-2026-0001".to_string(),
                    cvss_score: Some(9.8),
                    severity: Some("critical".to_string()),
                    fixed_version: Some("3.0.1".to_string()),
                    affected: true,
                    whitelisted: false,
                }],
            },
            result_digest_sha256:
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
            scan_duration_ms: 1234,
        };

        let json = serde_json::to_vec(&request).expect("completion should serialize");
        let decoded: CveScanCompleteRequest =
            serde_json::from_slice(&json).expect("completion should deserialize");

        assert_eq!(decoded, request);
    }

    #[test]
    fn canonical_cve_result_contract_has_exact_bytes_and_digest() {
        let result = CveScanResult {
            schema_version: CveScanSchemaVersion::V1,
            scanner: CveScannerIdentity {
                name: "vulnix".to_string(),
                version: "vulnix 1.12.4".to_string(),
            },
            derivation: CveScanDerivation {
                derivation_id: 42,
                derivation_name: "host-a".to_string(),
                drv_path: "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-host-a.drv".to_string(),
                outputs: vec![CveDerivationOutput {
                    name: "out".to_string(),
                    store_path: "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-host-a".to_string(),
                }],
            },
            entries: vec![CvePackageEvidence {
                entry_id: 0,
                package_name: "openssl".to_string(),
                package_version: Some("3.0.0".to_string()),
                drv_path: "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-openssl.drv".to_string(),
                outputs: vec![CveDerivationOutput {
                    name: "out".to_string(),
                    store_path: "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-openssl".to_string(),
                }],
            }],
            observations: vec![CveObservation {
                entry_id: 0,
                cve_id: "CVE-2026-0001".to_string(),
                cvss_score: Some(9.8),
                severity: Some("critical".to_string()),
                fixed_version: None,
                affected: true,
                whitelisted: false,
            }],
        };
        let bytes = canonical_cve_result_bytes(&result).expect("canonical bytes");
        assert_eq!(
            String::from_utf8(bytes).expect("canonical JSON is UTF-8"),
            r#"{"schema_version":1,"scanner":{"name":"vulnix","version":"vulnix 1.12.4"},"derivation":{"derivation_id":42,"derivation_name":"host-a","drv_path":"/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-host-a.drv","outputs":[{"name":"out","store_path":"/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-host-a"}]},"entries":[{"entry_id":0,"package_name":"openssl","package_version":"3.0.0","drv_path":"/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-openssl.drv","outputs":[{"name":"out","store_path":"/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-openssl"}]}],"observations":[{"entry_id":0,"cve_id":"CVE-2026-0001","cvss_score":9.8,"severity":"critical"}]}"#
        );
        assert_eq!(
            canonical_cve_result_digest(&result).expect("canonical digest"),
            "82424715e2a743c705a06ff2a771b486e77b958f9af3d388c9aa96914d56c71d"
        );
    }

    #[test]
    fn canonical_store_paths_reject_nested_traversal_and_extra_separators() {
        assert!(is_canonical_nix_store_path(
            "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package",
            false
        ));
        assert!(is_canonical_nix_store_path(
            "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv",
            true
        ));
        for invalid in [
            "/nix/store/../secret",
            "/nix/store/package/child",
            "/nix/store//package",
            "/nix/store/package/",
            "/nix/store/",
        ] {
            assert!(!is_canonical_nix_store_path(invalid, false), "{invalid}");
        }
        assert!(!is_canonical_nix_store_path(
            "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package",
            true
        ));
    }

    #[test]
    fn malformed_or_unknown_cve_schema_version_is_rejected() {
        for value in [
            serde_json::json!(0),
            serde_json::json!(2),
            serde_json::json!("1"),
        ] {
            assert!(
                serde_json::from_value::<CveScanSchemaVersion>(value).is_err(),
                "invalid schema version must be rejected"
            );
        }
    }

    #[test]
    fn server_issued_cve_policy_rejects_unbounded_limits() {
        let policy = serde_json::json!({
            "max_body_bytes": CVE_SCAN_MAX_BODY_BYTES + 1,
            "max_entries": CVE_SCAN_MAX_ENTRIES,
            "max_observations": CVE_SCAN_MAX_OBSERVATIONS,
            "timeout_seconds": 300,
            "scanner_args": []
        });

        assert!(serde_json::from_value::<CveScanPolicy>(policy).is_err());
    }

    #[test]
    fn legacy_observation_defaults_to_affected_and_not_whitelisted() {
        let observation: CveObservation = serde_json::from_value(serde_json::json!({
            "entry_id": 0,
            "cve_id": "CVE-2026-0001",
            "cvss_score": 7.5,
            "severity": "high",
            "fixed_version": null
        }))
        .expect("legacy schema-1 observation should remain valid");

        assert!(observation.affected);
        assert!(!observation.whitelisted);
        let encoded = serde_json::to_value(observation).expect("serialize observation");
        assert!(encoded.get("affected").is_none());
        assert!(encoded.get("whitelisted").is_none());
    }
}
