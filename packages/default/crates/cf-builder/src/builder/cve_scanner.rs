//! Executes server-authorized CVE scan leases on an API-only builder.
//!
//! The scanner uses only signed builder APIs and local Nix commands. It never
//! accesses the Crystal Forge database. Every child process has bounded output,
//! a timeout, and kill-on-drop behavior. Lease heartbeats fence materialization,
//! scanning, and package-output resolution.

use super::api_client::{BuilderApiClient, CveApiError};
use super::redaction::redact_builder_error;
use async_trait::async_trait;
use cf_protocol::builder::{
    BuilderCapabilities, CveDerivationOutput, CveObservation, CvePackageEvidence, CveScanClaim,
    CveScanCompleteRequest, CveScanDiagnostic, CveScanFailRequest, CveScanFailureClass,
    CveScanHeartbeatRequest, CveScanResult, CveScannerIdentity, canonical_cve_result_bytes,
    canonical_cve_result_digest, is_canonical_nix_store_path,
};
use chrono::Utc;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tracing::{info, warn};
use uuid::Uuid;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const MATERIALIZATION_TIMEOUT: Duration = Duration::from_secs(300);
const NIX_QUERY_TIMEOUT: Duration = Duration::from_secs(120);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const OUTPUT_DRAIN_GRACE: Duration = Duration::from_secs(1);
const STDERR_LIMIT: usize = 64 * 1024;
const COMMAND_OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
const NIX_STORE_PATH_OUTPUT_LIMIT: usize = 4096;
const DRV_QUERY_CHUNK: usize = 64;
// PERFORMANCE: Each pathless output requires a separate `nix-store` process.
// This limit and the shared deadline bound fallback work for the complete scan.
const PATHLESS_OUTPUTS_PER_RESOLUTION: usize = 256;

/// Failure produced while executing a CVE lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CveScanExecutionError {
    /// Protocol class reported to the server.
    pub class: CveScanFailureClass,
    /// Bounded credential-free diagnostic.
    pub message: String,
    /// Bounded command diagnostics collected before the failure.
    pub diagnostics: Vec<CveScanDiagnostic>,
}

impl CveScanExecutionError {
    fn new(class: CveScanFailureClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into().chars().take(2048).collect(),
            diagnostics: Vec::new(),
        }
    }

    fn with_diagnostic(mut self, diagnostic: CveScanDiagnostic) -> Self {
        self.diagnostics.push(diagnostic);
        self
    }

    fn with_diagnostics(mut self, diagnostics: Vec<CveScanDiagnostic>) -> Self {
        let mut combined = diagnostics;
        combined.append(&mut self.diagnostics);
        self.diagnostics = combined;
        self
    }
}

/// Signed API operations required by one scanner execution.
#[async_trait]
pub trait CveLeaseApi: Send + Sync {
    /// Renews an active lease and returns whether execution can continue.
    ///
    /// # Errors
    ///
    /// Returns a classified API error when the heartbeat is not accepted.
    async fn heartbeat(&self, request: &CveScanHeartbeatRequest) -> Result<bool, CveApiError>;

    /// Completes an active lease with canonical evidence.
    ///
    /// # Errors
    ///
    /// Returns a classified API error when completion is not accepted.
    async fn complete(&self, request: &CveScanCompleteRequest) -> Result<(), CveApiError>;

    /// Reports a classified failure for an active lease.
    ///
    /// # Errors
    ///
    /// Returns a classified API error when the failure report is not accepted.
    async fn fail(&self, request: &CveScanFailRequest) -> Result<(), CveApiError>;
}

#[async_trait]
impl CveLeaseApi for BuilderApiClient {
    async fn heartbeat(&self, request: &CveScanHeartbeatRequest) -> Result<bool, CveApiError> {
        self.heartbeat_cve_scan(request)
            .await
            .map(|response| !response.revocation_requested)
    }

    async fn complete(&self, request: &CveScanCompleteRequest) -> Result<(), CveApiError> {
        self.complete_cve_scan(request).await.map(|_| ())
    }

    async fn fail(&self, request: &CveScanFailRequest) -> Result<(), CveApiError> {
        self.fail_cve_scan(request).await.map(|_| ())
    }
}

/// Returns scanner capabilities only when configuration and the executable
/// probe both permit local vulnix execution.
pub async fn detect_cve_capabilities(enabled: bool) -> BuilderCapabilities {
    if !enabled {
        return BuilderCapabilities::default();
    }
    let args = vec!["--version".to_string()];
    match run_probe("vulnix", &args, PROBE_TIMEOUT, 4096).await {
        Ok(result) if result.status_code == Some(0) => {
            let version = String::from_utf8_lossy(&result.stdout).trim().to_string();
            if version.is_empty() {
                BuilderCapabilities::default()
            } else {
                BuilderCapabilities::current_cve_scanner(version)
            }
        }
        _ => BuilderCapabilities::default(),
    }
}

/// Executes and reports one claimed scan without changing its producing build.
pub async fn execute_claim<A: CveLeaseApi>(
    api: &A,
    claim: CveScanClaim,
    local_scanner: CveScannerIdentity,
) {
    let started = Instant::now();
    match execute_claim_inner(api, &claim, &local_scanner).await {
        Ok((result, mut diagnostics)) => {
            diagnostics.push(scan_diagnostic(
                "info",
                "builder",
                "attempt_completed",
                "Remote CVE scan attempt completed.",
                false,
            ));
            let encoded = match canonical_cve_result_bytes(&result) {
                Ok(encoded) => encoded,
                Err(_) => {
                    report_failure(
                        api,
                        &claim,
                        CveScanExecutionError::new(
                            CveScanFailureClass::Deterministic,
                            "failed to encode canonical CVE result",
                        ),
                    )
                    .await;
                    return;
                }
            };
            if encoded.len() as u64 > claim.policy.max_body_bytes {
                report_failure(
                    api,
                    &claim,
                    CveScanExecutionError::new(
                        CveScanFailureClass::Deterministic,
                        "canonical CVE result exceeds the authorized body limit",
                    ),
                )
                .await;
                return;
            }
            let result_digest_sha256 = match canonical_cve_result_digest(&result) {
                Ok(digest) => digest,
                Err(_) => {
                    report_failure(
                        api,
                        &claim,
                        CveScanExecutionError::new(
                            CveScanFailureClass::Deterministic,
                            "failed to hash canonical CVE result",
                        ),
                    )
                    .await;
                    return;
                }
            };
            let request = CveScanCompleteRequest {
                lease: claim.lease,
                result,
                result_digest_sha256,
                scan_duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                diagnostics,
            };
            let request_size = serde_json::to_vec(&request)
                .map(|body| body.len() as u64)
                .unwrap_or(u64::MAX);
            if request_size > claim.policy.max_body_bytes {
                report_failure(
                    api,
                    &claim,
                    CveScanExecutionError::new(
                        CveScanFailureClass::Deterministic,
                        "CVE completion request exceeds the authorized body limit",
                    ),
                )
                .await;
                return;
            }
            match api.complete(&request).await {
                Ok(()) => info!(scan_id = %claim.lease.scan_id, "CVE scan lease completed"),
                Err(CveApiError::Revoked) => {
                    info!(scan_id = %claim.lease.scan_id, "CVE scan lease was revoked before completion")
                }
                Err(error) => warn!(
                    scan_id = %claim.lease.scan_id,
                    class = ?error.failure_class(),
                    "CVE completion report failed"
                ),
            }
        }
        Err(error) => report_failure(api, &claim, error).await,
    }
}

async fn report_failure<A: CveLeaseApi>(
    api: &A,
    claim: &CveScanClaim,
    error: CveScanExecutionError,
) {
    if error.class == CveScanFailureClass::Cancelled {
        info!(scan_id = %claim.lease.scan_id, "CVE scan stopped after lease revocation");
    }
    let request = CveScanFailRequest {
        lease: claim.lease,
        failure_class: error.class,
        error_message: error.message,
        diagnostics: error.diagnostics,
    };
    if let Err(report_error) = api.fail(&request).await
        && report_error != CveApiError::Revoked
    {
        warn!(
            scan_id = %claim.lease.scan_id,
            class = ?report_error.failure_class(),
            "CVE failure report failed"
        );
    }
}

async fn execute_claim_inner<A: CveLeaseApi>(
    api: &A,
    claim: &CveScanClaim,
    local_scanner: &CveScannerIdentity,
) -> Result<(CveScanResult, Vec<CveScanDiagnostic>), CveScanExecutionError> {
    if claim.scanner != *local_scanner {
        return Err(CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            "claimed scanner identity does not match the local scanner probe",
        ));
    }
    if claim.scanner.name != "vulnix" || claim.derivation.outputs.is_empty() {
        return Err(CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            "unsupported scanner or empty authorized output set",
        ));
    }
    let entries = Arc::new(AtomicUsize::new(0));
    let observations = Arc::new(AtomicUsize::new(0));
    let mut diagnostics = vec![scan_diagnostic(
        "info",
        "builder",
        "attempt_started",
        "Remote CVE scan attempt started.",
        false,
    )];

    for output in &claim.derivation.outputs {
        validate_store_path(&output.store_path, false)?;
        if !tokio::fs::try_exists(&output.store_path)
            .await
            .unwrap_or(false)
        {
            let args = vec!["--realise".to_string(), output.store_path.clone()];
            let result = run_leased_command(
                api,
                claim,
                "nix-store",
                &args,
                MATERIALIZATION_TIMEOUT,
                COMMAND_OUTPUT_LIMIT,
                Arc::clone(&entries),
                Arc::clone(&observations),
            )
            .await
            .map_err(|error| error.with_diagnostics(diagnostics.clone()))?;
            if result.status_code != Some(0)
                || !tokio::fs::try_exists(&output.store_path)
                    .await
                    .unwrap_or(false)
            {
                let mut error = CveScanExecutionError::new(
                    CveScanFailureClass::Transient,
                    "failed to materialize an authorized scan output",
                )
                .with_diagnostics(diagnostics.clone());
                if !result.stderr.is_empty() {
                    error = error.with_diagnostic(scan_diagnostic(
                        "error",
                        "nix",
                        "output",
                        &String::from_utf8_lossy(&result.stderr),
                        result.stderr_overflow,
                    ));
                }
                return Err(error);
            }
        }
    }

    let mut scanner_args = claim.policy.scanner_args.clone();
    scanner_args.extend(
        claim
            .derivation
            .outputs
            .iter()
            .map(|output| output.store_path.clone()),
    );
    let timeout = Duration::from_secs(claim.policy.timeout_seconds.max(1));
    let scan = run_leased_command(
        api,
        claim,
        "vulnix",
        &scanner_args,
        timeout,
        usize::try_from(claim.policy.max_body_bytes).unwrap_or(COMMAND_OUTPUT_LIMIT),
        Arc::clone(&entries),
        Arc::clone(&observations),
    )
    .await
    .map_err(|error| error.with_diagnostics(diagnostics.clone()))?;
    if !scan.stderr.is_empty() {
        diagnostics.push(scan_diagnostic(
            if matches!(scan.status_code, Some(0 | 2)) {
                "warning"
            } else {
                "error"
            },
            "vulnix",
            "output",
            &String::from_utf8_lossy(&scan.stderr),
            scan.stderr_overflow,
        ));
    }
    let parsed = parse_vulnix_output(scan.status_code, &scan.stdout, scan.stdout_overflow)
        .map_err(|error| error.with_diagnostics(diagnostics.clone()))?;
    if parsed.len() > claim.policy.max_entries {
        return Err(CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            "vulnix result exceeds the authorized entry limit",
        ));
    }

    entries.store(parsed.len(), Ordering::Relaxed);
    let result = canonical_result(api, claim, parsed, entries, observations)
        .await
        .map_err(|error| error.with_diagnostics(diagnostics.clone()))?;
    let heartbeat = CveScanHeartbeatRequest {
        lease: claim.lease,
        entries_collected: result.entries.len(),
        observations_collected: result.observations.len(),
    };
    match api.heartbeat(&heartbeat).await {
        Ok(true) => Ok((result, diagnostics)),
        Ok(false) | Err(CveApiError::Revoked) => Err(CveScanExecutionError::new(
            CveScanFailureClass::Cancelled,
            "CVE scan lease was revoked",
        )
        .with_diagnostics(diagnostics)),
        Err(error) => Err(CveScanExecutionError::new(
            error.failure_class(),
            "CVE scan heartbeat failed",
        )
        .with_diagnostics(diagnostics)),
    }
}

#[derive(Debug, Deserialize)]
struct VulnixEntry {
    name: String,
    pname: String,
    version: String,
    affected_by: Vec<String>,
    #[serde(default)]
    whitelisted: Vec<String>,
    derivation: String,
    #[serde(default)]
    cvssv3_basescore: BTreeMap<String, f32>,
}

fn parse_vulnix_output(
    status_code: Option<i32>,
    stdout: &[u8],
    overflow: bool,
) -> Result<Vec<VulnixEntry>, CveScanExecutionError> {
    if overflow {
        return Err(CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            "vulnix stdout exceeded the authorized limit",
        ));
    }
    if !matches!(status_code, Some(0 | 2)) {
        return Err(CveScanExecutionError::new(
            CveScanFailureClass::Transient,
            "vulnix process failed",
        ));
    }
    serde_json::from_slice(stdout).map_err(|_| {
        CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            "vulnix returned malformed JSON",
        )
    })
}

async fn canonical_result<A: CveLeaseApi>(
    api: &A,
    claim: &CveScanClaim,
    mut parsed: Vec<VulnixEntry>,
    entries_count: Arc<AtomicUsize>,
    observations_count: Arc<AtomicUsize>,
) -> Result<CveScanResult, CveScanExecutionError> {
    parsed.sort_by(|a, b| {
        a.derivation
            .cmp(&b.derivation)
            .then(a.pname.cmp(&b.pname))
            .then(a.version.cmp(&b.version))
            .then(a.name.cmp(&b.name))
    });
    let drv_paths: Vec<String> = parsed
        .iter()
        .map(|entry| entry.derivation.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let outputs = resolve_drv_outputs(
        api,
        claim,
        &drv_paths,
        Arc::clone(&entries_count),
        Arc::clone(&observations_count),
    )
    .await?;

    let result = assemble_result(claim, parsed, outputs)?;
    observations_count.store(result.observations.len(), Ordering::Relaxed);
    Ok(result)
}

fn assemble_result(
    claim: &CveScanClaim,
    mut parsed: Vec<VulnixEntry>,
    outputs: BTreeMap<String, Vec<CveDerivationOutput>>,
) -> Result<CveScanResult, CveScanExecutionError> {
    parsed.sort_by(|a, b| {
        a.derivation
            .cmp(&b.derivation)
            .then(a.pname.cmp(&b.pname))
            .then(a.version.cmp(&b.version))
            .then(a.name.cmp(&b.name))
    });
    let mut evidence = Vec::with_capacity(parsed.len());
    let mut observations = Vec::new();
    for (index, entry) in parsed.into_iter().enumerate() {
        validate_store_path(&entry.derivation, true)?;
        let entry_id = u32::try_from(index).map_err(|_| {
            CveScanExecutionError::new(
                CveScanFailureClass::Deterministic,
                "vulnix returned too many entries",
            )
        })?;
        let package_outputs = outputs.get(&entry.derivation).cloned().ok_or_else(|| {
            CveScanExecutionError::new(
                CveScanFailureClass::Deterministic,
                "Nix did not return package derivation outputs",
            )
        })?;
        evidence.push(CvePackageEvidence {
            entry_id,
            package_name: if entry.pname.trim().is_empty() {
                entry.name
            } else {
                entry.pname
            },
            package_version: (!entry.version.is_empty()).then_some(entry.version),
            drv_path: entry.derivation,
            outputs: package_outputs,
        });

        let affected = entry
            .affected_by
            .into_iter()
            .map(|value| canonical_cve(&value))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let whitelisted = entry
            .whitelisted
            .into_iter()
            .map(|value| canonical_cve(&value))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let scores = entry
            .cvssv3_basescore
            .into_iter()
            .map(|(cve, score)| canonical_cve(&cve).map(|cve| (cve, score)))
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        for cve_id in affected.union(&whitelisted) {
            let score = scores.get(cve_id).copied();
            if score.is_some_and(|value| !value.is_finite() || !(0.0..=10.0).contains(&value)) {
                return Err(CveScanExecutionError::new(
                    CveScanFailureClass::Deterministic,
                    "vulnix returned an invalid CVSS score",
                ));
            }
            observations.push(CveObservation {
                entry_id,
                cve_id: cve_id.clone(),
                cvss_score: score,
                severity: score.map(severity_for_score),
                fixed_version: None,
                affected: affected.contains(cve_id),
                whitelisted: whitelisted.contains(cve_id),
            });
        }
    }
    observations.sort_by(|a, b| a.entry_id.cmp(&b.entry_id).then(a.cve_id.cmp(&b.cve_id)));
    if observations.len() > claim.policy.max_observations {
        return Err(CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            "vulnix result exceeds the authorized observation limit",
        ));
    }
    Ok(CveScanResult {
        schema_version: claim.schema_version,
        scanner: claim.scanner.clone(),
        derivation: claim.derivation.clone(),
        entries: evidence,
        observations,
    })
}

fn canonical_cve(value: &str) -> Result<String, CveScanExecutionError> {
    let value = value.trim().to_ascii_uppercase();
    let mut parts = value.split('-');
    let valid = value.len() <= 20
        && parts.next() == Some("CVE")
        && parts
            .next()
            .is_some_and(|part| part.len() == 4 && part.bytes().all(|byte| byte.is_ascii_digit()))
        && parts
            .next()
            .is_some_and(|part| part.len() >= 4 && part.bytes().all(|byte| byte.is_ascii_digit()))
        && parts.next().is_none();
    if !valid {
        return Err(CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            "vulnix returned an invalid CVE identifier",
        ));
    }
    Ok(value)
}

fn severity_for_score(score: f32) -> String {
    match score {
        value if value >= 9.0 => "critical",
        value if value >= 7.0 => "high",
        value if value >= 4.0 => "medium",
        value if value > 0.0 => "low",
        _ => "unknown",
    }
    .to_string()
}

async fn resolve_drv_outputs<A: CveLeaseApi>(
    api: &A,
    claim: &CveScanClaim,
    drv_paths: &[String],
    entries: Arc<AtomicUsize>,
    observations: Arc<AtomicUsize>,
) -> Result<BTreeMap<String, Vec<CveDerivationOutput>>, CveScanExecutionError> {
    resolve_drv_outputs_with_programs(
        api,
        claim,
        drv_paths,
        entries,
        observations,
        "nix",
        "nix-store",
    )
    .await
}

async fn resolve_drv_outputs_with_programs<A: CveLeaseApi>(
    api: &A,
    claim: &CveScanClaim,
    drv_paths: &[String],
    entries: Arc<AtomicUsize>,
    observations: Arc<AtomicUsize>,
    nix_program: &str,
    nix_store_program: &str,
) -> Result<BTreeMap<String, Vec<CveDerivationOutput>>, CveScanExecutionError> {
    let resolution_deadline = Instant::now() + NIX_QUERY_TIMEOUT;
    let mut pathless_output_count = 0usize;
    let mut resolved = BTreeMap::new();
    for chunk in drv_paths.chunks(DRV_QUERY_CHUNK) {
        let mut args = vec!["derivation".to_string(), "show".to_string()];
        args.extend(chunk.iter().cloned());
        let output = run_leased_command(
            api,
            claim,
            nix_program,
            &args,
            remaining_resolution_time(resolution_deadline)?,
            COMMAND_OUTPUT_LIMIT,
            Arc::clone(&entries),
            Arc::clone(&observations),
        )
        .await?;
        if output.status_code != Some(0) || output.stdout_overflow {
            return Err(CveScanExecutionError::new(
                CveScanFailureClass::Transient,
                "Nix package-output resolution failed",
            ));
        }
        let parsed = normalize_derivation_show_output(&output.stdout, chunk)?;
        pathless_output_count = add_pathless_output_count(pathless_output_count, &parsed)?;
        resolved.extend(
            resolve_missing_derivation_outputs(
                api,
                claim,
                parsed,
                entries.clone(),
                observations.clone(),
                nix_store_program,
                resolution_deadline,
            )
            .await?,
        );
    }
    Ok(resolved)
}

#[derive(Debug, Deserialize)]
struct DerivationShowDocumentV4 {
    version: u64,
    derivations: BTreeMap<String, DerivationShowEntryV4>,
}

#[derive(Debug, Deserialize)]
struct DerivationShowEntryV4 {
    version: u64,
    outputs: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedDerivationOutput {
    name: String,
    store_path: Option<String>,
}

fn normalize_derivation_show_output(
    stdout: &[u8],
    requested: &[String],
) -> Result<BTreeMap<String, Vec<ParsedDerivationOutput>>, CveScanExecutionError> {
    if requested.len() > DRV_QUERY_CHUNK {
        return Err(CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            "Nix package-output resolution exceeded the derivation chunk limit",
        ));
    }
    let mut requested_by_base_name = BTreeMap::new();
    for drv_path in requested {
        validate_store_path(drv_path, true)?;
        let base_name = drv_path.strip_prefix("/nix/store/").ok_or_else(|| {
            CveScanExecutionError::new(
                CveScanFailureClass::Deterministic,
                "scanner returned an invalid Nix store path",
            )
        })?;
        if requested_by_base_name
            .insert(base_name.to_string(), drv_path.clone())
            .is_some()
        {
            return Err(CveScanExecutionError::new(
                CveScanFailureClass::Deterministic,
                "Nix package-output resolution received duplicate derivations",
            ));
        }
    }

    let document: DerivationShowDocumentV4 = serde_json::from_slice(stdout).map_err(|_| {
        CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            "Nix package-output resolution returned malformed JSON",
        )
    })?;
    if document.version != 4
        || document
            .derivations
            .values()
            .any(|derivation| derivation.version != 4)
    {
        return Err(CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            "Nix package-output resolution returned an unsupported version",
        ));
    }
    for base_name in document.derivations.keys() {
        normalize_store_path_base_name(base_name, true)?;
    }
    let expected = requested_by_base_name.keys().collect::<BTreeSet<_>>();
    let actual = document.derivations.keys().collect::<BTreeSet<_>>();
    let missing = expected.difference(&actual).count();
    let extra = actual.difference(&expected).count();
    if missing != 0 || extra != 0 {
        return Err(CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            format!(
                "Nix returned a mismatched derivation set (missing={missing}, extra={extra}, entries={}, requested={})",
                document.derivations.len(),
                requested.len()
            ),
        ));
    }

    let mut normalized = BTreeMap::new();
    for (base_name, derivation) in document.derivations {
        let drv_path = requested_by_base_name.remove(&base_name).ok_or_else(|| {
            CveScanExecutionError::new(
                CveScanFailureClass::Deterministic,
                "Nix returned a mismatched derivation set",
            )
        })?;
        if derivation.outputs.is_empty() {
            return Err(CveScanExecutionError::new(
                CveScanFailureClass::Deterministic,
                "package derivation has no outputs",
            ));
        }
        let mut package_outputs = Vec::with_capacity(derivation.outputs.len());
        for (name, output) in derivation.outputs {
            let output = output.as_object().ok_or_else(|| {
                CveScanExecutionError::new(
                    CveScanFailureClass::Deterministic,
                    "Nix package-output resolution returned a malformed output entry",
                )
            })?;
            let store_path = match output.get("path") {
                Some(path) => {
                    let base_name = path.as_str().filter(|path| !path.is_empty()).ok_or_else(
                        || {
                            CveScanExecutionError::new(
                                CveScanFailureClass::Deterministic,
                                "Nix package-output resolution returned a malformed output path",
                            )
                        },
                    )?;
                    Some(normalize_store_path_base_name(base_name, false)?)
                }
                None => None,
            };
            package_outputs.push(ParsedDerivationOutput { name, store_path });
        }
        package_outputs.sort_by(|a, b| a.name.cmp(&b.name).then(a.store_path.cmp(&b.store_path)));
        normalized.insert(drv_path, package_outputs);
    }
    Ok(normalized)
}

fn add_pathless_output_count(
    current: usize,
    parsed: &BTreeMap<String, Vec<ParsedDerivationOutput>>,
) -> Result<usize, CveScanExecutionError> {
    let additional = parsed
        .values()
        .flatten()
        .filter(|output| output.store_path.is_none())
        .count();
    let total = current.checked_add(additional).ok_or_else(|| {
        CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            "Nix package-output resolution exceeded the pathless-output limit",
        )
    })?;
    if total > PATHLESS_OUTPUTS_PER_RESOLUTION {
        return Err(CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            "Nix package-output resolution exceeded the pathless-output limit",
        ));
    }
    Ok(total)
}

fn remaining_resolution_time(deadline: Instant) -> Result<Duration, CveScanExecutionError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| {
            CveScanExecutionError::new(
                CveScanFailureClass::Transient,
                "Nix package-output resolution exceeded its aggregate timeout",
            )
        })
}

fn normalize_store_path_base_name(
    base_name: &str,
    derivation: bool,
) -> Result<String, CveScanExecutionError> {
    if base_name.is_empty() || base_name.contains('/') {
        return Err(CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            "Nix package-output resolution returned an invalid store-path base name",
        ));
    }
    let store_path = format!("/nix/store/{base_name}");
    validate_store_path(&store_path, derivation)?;
    Ok(store_path)
}

async fn resolve_missing_derivation_outputs<A: CveLeaseApi>(
    api: &A,
    claim: &CveScanClaim,
    parsed: BTreeMap<String, Vec<ParsedDerivationOutput>>,
    entries: Arc<AtomicUsize>,
    observations: Arc<AtomicUsize>,
    nix_store_program: &str,
    resolution_deadline: Instant,
) -> Result<BTreeMap<String, Vec<CveDerivationOutput>>, CveScanExecutionError> {
    let mut resolved = BTreeMap::new();
    for (drv_path, outputs) in parsed {
        let mut package_outputs = Vec::with_capacity(outputs.len());
        for output in outputs {
            let store_path = match output.store_path {
                Some(store_path) => store_path,
                None => {
                    let args = vec![
                        "--query".to_string(),
                        "--binding".to_string(),
                        output.name.clone(),
                        drv_path.clone(),
                    ];
                    let result = run_leased_command(
                        api,
                        claim,
                        nix_store_program,
                        &args,
                        remaining_resolution_time(resolution_deadline)?,
                        NIX_STORE_PATH_OUTPUT_LIMIT,
                        entries.clone(),
                        observations.clone(),
                    )
                    .await?;
                    if result.status_code != Some(0) || result.stdout_overflow {
                        return Err(CveScanExecutionError::new(
                            CveScanFailureClass::Transient,
                            "Nix package-output binding resolution failed",
                        ));
                    }
                    let stdout = std::str::from_utf8(&result.stdout).map_err(|_| {
                        CveScanExecutionError::new(
                            CveScanFailureClass::Deterministic,
                            "Nix package-output binding returned invalid text",
                        )
                    })?;
                    let mut paths = stdout
                        .lines()
                        .map(str::trim)
                        .filter(|path| !path.is_empty());
                    let store_path = paths.next().ok_or_else(|| {
                        CveScanExecutionError::new(
                            CveScanFailureClass::Deterministic,
                            "Nix package-output binding returned no store path",
                        )
                    })?;
                    if paths.next().is_some() {
                        return Err(CveScanExecutionError::new(
                            CveScanFailureClass::Deterministic,
                            "Nix package-output binding returned multiple store paths",
                        ));
                    }
                    validate_store_path(store_path, false)?;
                    store_path.to_string()
                }
            };
            package_outputs.push(CveDerivationOutput {
                name: output.name,
                store_path,
            });
        }
        package_outputs.sort_by(|a, b| a.name.cmp(&b.name).then(a.store_path.cmp(&b.store_path)));
        if package_outputs.is_empty() {
            return Err(CveScanExecutionError::new(
                CveScanFailureClass::Deterministic,
                "package derivation has no resolved outputs",
            ));
        }
        resolved.insert(drv_path, package_outputs);
    }
    Ok(resolved)
}

fn validate_store_path(value: &str, derivation: bool) -> Result<(), CveScanExecutionError> {
    if !is_canonical_nix_store_path(value, derivation) {
        return Err(CveScanExecutionError::new(
            CveScanFailureClass::Deterministic,
            "scanner returned an invalid Nix store path",
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct CommandResult {
    status_code: Option<i32>,
    stdout: Vec<u8>,
    stdout_overflow: bool,
    stderr: Vec<u8>,
    stderr_overflow: bool,
}

#[derive(Debug, Default)]
struct BoundedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

type SharedBoundedOutput = Arc<Mutex<BoundedOutput>>;

fn bounded_output_snapshot(output: &SharedBoundedOutput) -> (Vec<u8>, bool) {
    let output = output
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    (output.bytes.clone(), output.truncated)
}

async fn finish_output_reader(
    task: &mut tokio::task::JoinHandle<Result<(), CveScanExecutionError>>,
    output: &SharedBoundedOutput,
) -> Result<(Vec<u8>, bool), CveScanExecutionError> {
    task.await.map_err(|_| {
        CveScanExecutionError::new(
            CveScanFailureClass::Transient,
            "failed to collect bounded CVE scan output",
        )
    })??;
    Ok(bounded_output_snapshot(output))
}

async fn capture_terminated_output(
    task: &mut tokio::task::JoinHandle<Result<(), CveScanExecutionError>>,
    output: &SharedBoundedOutput,
) -> (Vec<u8>, bool) {
    if tokio::time::timeout(OUTPUT_DRAIN_GRACE, &mut *task)
        .await
        .is_err()
    {
        task.abort();
    }
    bounded_output_snapshot(output)
}

fn scan_diagnostic(
    level: &str,
    source: &str,
    event_type: &str,
    message: &str,
    truncated: bool,
) -> CveScanDiagnostic {
    let message = redact_builder_error(message);
    CveScanDiagnostic {
        occurred_at: Utc::now(),
        level: level.to_string(),
        source: source.to_string(),
        event_type: event_type.to_string(),
        message: message.chars().take(STDERR_LIMIT).collect(),
        truncated: truncated || message.chars().count() > STDERR_LIMIT,
    }
}

/// Owns one isolated Unix process group and its direct child.
///
/// CONCURRENCY: `Drop` sends `SIGKILL` synchronously before control returns to
/// the caller. This prevents a cancelled scan future or runtime shutdown from
/// releasing its scanner slot while a descendant still runs. Reaping the direct
/// child is deferred only after the group has been signalled. Descendants are
/// not children of the builder and are reaped by their Unix subreaper or init.
struct ProcessGroupGuard {
    child: Option<tokio::process::Child>,
    pgid: i32,
    child_reaped: bool,
    armed: bool,
}

impl ProcessGroupGuard {
    fn new(child: tokio::process::Child) -> Result<Self, CveScanExecutionError> {
        let pgid = child
            .id()
            .and_then(|pid| i32::try_from(pid).ok())
            .ok_or_else(|| {
                CveScanExecutionError::new(
                    CveScanFailureClass::Transient,
                    "CVE scan child has no valid process group ID",
                )
            })?;
        Ok(Self {
            child: Some(child),
            pgid,
            child_reaped: false,
            armed: true,
        })
    }

    fn child_mut(&mut self) -> &mut tokio::process::Child {
        self.child
            .as_mut()
            .expect("process-group child is present while guard is armed")
    }

    async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        let status = self.child_mut().wait().await;
        if status.is_ok() {
            self.child_reaped = true;
        }
        status
    }

    async fn terminate(&mut self) {
        self.armed = false;
        signal_process_group(self.pgid);
        if !self.child_reaped
            && let Some(mut child) = self.child.take()
        {
            #[cfg(not(unix))]
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
    }

    fn disarm(&mut self) {
        debug_assert!(self.child_reaped);
        self.armed = false;
        self.child.take();
    }
}

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        signal_process_group(self.pgid);
        if self.child_reaped {
            return;
        }
        let Some(mut child) = self.child.take() else {
            return;
        };
        #[cfg(not(unix))]
        let _ = child.start_kill();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = child.wait().await;
            });
        } else {
            let _ = child.start_kill();
        }
    }
}

#[cfg(unix)]
fn signal_process_group(pgid: i32) {
    use nix::errno::Errno;
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;

    if let Err(error) = killpg(Pid::from_raw(pgid), Signal::SIGKILL)
        && error != Errno::ESRCH
    {
        warn!(pgid, %error, "failed to terminate CVE scan process group");
    }
}

#[cfg(not(unix))]
fn signal_process_group(_pgid: i32) {}

async fn run_probe(
    program: &str,
    args: &[String],
    timeout: Duration,
    stdout_limit: usize,
) -> Result<CommandResult, CveScanExecutionError> {
    run_command(program, args, timeout, stdout_limit, || async { Ok(()) }).await
}

async fn run_leased_command<A: CveLeaseApi>(
    api: &A,
    claim: &CveScanClaim,
    program: &str,
    args: &[String],
    timeout: Duration,
    stdout_limit: usize,
    entries: Arc<AtomicUsize>,
    observations: Arc<AtomicUsize>,
) -> Result<CommandResult, CveScanExecutionError> {
    run_command(program, args, timeout, stdout_limit, || {
        let heartbeat = CveScanHeartbeatRequest {
            lease: claim.lease,
            entries_collected: entries.load(Ordering::Relaxed),
            observations_collected: observations.load(Ordering::Relaxed),
        };
        async move {
            match api.heartbeat(&heartbeat).await {
                Ok(true) => Ok(()),
                Ok(false) | Err(CveApiError::Revoked) => Err(CveScanExecutionError::new(
                    CveScanFailureClass::Cancelled,
                    "CVE scan lease was revoked",
                )),
                Err(error) => Err(CveScanExecutionError::new(
                    error.failure_class(),
                    "CVE scan heartbeat failed",
                )),
            }
        }
    })
    .await
}

async fn run_command<F, Fut>(
    program: &str,
    args: &[String],
    timeout: Duration,
    stdout_limit: usize,
    mut heartbeat: F,
) -> Result<CommandResult, CveScanExecutionError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), CveScanExecutionError>>,
{
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let child = command.spawn().map_err(|_| {
        CveScanExecutionError::new(
            CveScanFailureClass::Transient,
            "failed to start a CVE scan child process",
        )
    })?;
    let mut child = ProcessGroupGuard::new(child)?;
    let stdout = child.child_mut().stdout.take().ok_or_else(|| {
        CveScanExecutionError::new(
            CveScanFailureClass::Transient,
            "failed to capture CVE scan stdout",
        )
    })?;
    let stderr = child.child_mut().stderr.take().ok_or_else(|| {
        CveScanExecutionError::new(
            CveScanFailureClass::Transient,
            "failed to capture CVE scan stderr",
        )
    })?;
    let stdout_output = Arc::new(Mutex::new(BoundedOutput::default()));
    let stderr_output = Arc::new(Mutex::new(BoundedOutput::default()));
    let mut stdout_task = tokio::spawn(read_bounded(
        stdout,
        stdout_limit,
        Arc::clone(&stdout_output),
    ));
    let mut stderr_task = tokio::spawn(read_bounded(
        stderr,
        STDERR_LIMIT,
        Arc::clone(&stderr_output),
    ));
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);
    let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let status = loop {
        tokio::select! {
            status = child.wait() => break status.map_err(|_| {
                CveScanExecutionError::new(
                    CveScanFailureClass::Transient,
                    "failed while waiting for a CVE scan child process",
                )
            })?,
            _ = &mut deadline => {
                child.terminate().await;
                stdout_task.abort();
                let (stderr, stderr_overflow) =
                    capture_terminated_output(&mut stderr_task, &stderr_output).await;
                let mut error = CveScanExecutionError::new(
                    CveScanFailureClass::Transient,
                    "CVE scan child process timed out",
                );
                if !stderr.is_empty() {
                    error = error.with_diagnostic(scan_diagnostic(
                        "error",
                        "builder",
                        "output",
                        &String::from_utf8_lossy(&stderr),
                        stderr_overflow,
                    ));
                }
                return Err(error.with_diagnostic(scan_diagnostic(
                    "error",
                    "builder",
                    "attempt_failed",
                    "CVE scan child process timed out and its process group was terminated.",
                    false,
                )));
            }
            _ = ticker.tick() => {
                if let Err(error) = heartbeat().await {
                    child.terminate().await;
                    stdout_task.abort();
                    stderr_task.abort();
                    return Err(error);
                }
            }
        }
    };
    let (stdout, stdout_overflow) = tokio::select! {
        result = finish_output_reader(&mut stdout_task, &stdout_output) => result?,
        _ = &mut deadline => {
            child.terminate().await;
            stdout_task.abort();
            stderr_task.abort();
            return Err(CveScanExecutionError::new(
                CveScanFailureClass::Transient,
                "CVE scan output drain timed out",
            ));
        }
    };
    let (stderr, stderr_overflow) = tokio::select! {
        result = finish_output_reader(&mut stderr_task, &stderr_output) => result?,
        _ = &mut deadline => {
            child.terminate().await;
            stderr_task.abort();
            return Err(CveScanExecutionError::new(
                CveScanFailureClass::Transient,
                "CVE scan output drain timed out",
            ));
        }
    };
    child.disarm();
    Ok(CommandResult {
        status_code: status.code(),
        stdout,
        stdout_overflow,
        stderr,
        stderr_overflow,
    })
}

async fn read_bounded<R: tokio::io::AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
    output: SharedBoundedOutput,
) -> Result<(), CveScanExecutionError> {
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader.read(&mut buffer).await.map_err(|_| {
            CveScanExecutionError::new(
                CveScanFailureClass::Transient,
                "failed to read CVE scan child output",
            )
        })?;
        if count == 0 {
            break;
        }
        let mut output = output
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let available = limit.saturating_sub(output.bytes.len());
        output
            .bytes
            .extend_from_slice(&buffer[..count.min(available)]);
        output.truncated |= count > available;
    }
    Ok(())
}

/// Attempts one affinity or background claim when scanner capability is active.
pub async fn claim_and_execute(
    client: &BuilderApiClient,
    capabilities: BuilderCapabilities,
    completed_build_job_id: Option<Uuid>,
) {
    let Some(local_scanner) = capabilities.cve_scanner.clone() else {
        return;
    };
    if !capabilities.supports_current_cve_schema() {
        return;
    }
    match client
        .claim_cve_scan(capabilities, completed_build_job_id)
        .await
    {
        Ok(response) => {
            if let Some(claim) = response.claim {
                execute_claim(client, claim, local_scanner).await;
            }
        }
        Err(CveApiError::Revoked) => warn!("builder session was revoked during CVE claim"),
        Err(error) => warn!(class = ?error.failure_class(), "CVE claim failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_protocol::builder::{
        CVE_SCAN_MAX_BODY_BYTES, CVE_SCAN_MAX_ENTRIES, CVE_SCAN_MAX_OBSERVATIONS,
        CveScanDerivation, CveScanLease, CveScanPolicy, CveScanSchemaVersion, CveScannerIdentity,
    };
    use chrono::{Duration as ChronoDuration, Utc};

    fn vulnix_json() -> Vec<u8> {
        br#"[{"name":"zlib-1.3","pname":"zlib","version":"1.3","affected_by":["CVE-2026-0002"],"whitelisted":["CVE-2026-0001"],"derivation":"/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-zlib.drv","cvssv3_basescore":{"CVE-2026-0002":7.5}}]"#.to_vec()
    }

    fn claim() -> CveScanClaim {
        CveScanClaim {
            lease: CveScanLease {
                scan_id: Uuid::new_v4(),
                execution_id: Uuid::new_v4(),
                builder_id: Uuid::new_v4(),
                builder_session_id: Uuid::new_v4(),
            },
            derivation: CveScanDerivation {
                derivation_id: 1,
                derivation_name: "system".to_string(),
                drv_path: "/nix/store/cccccccccccccccccccccccccccccccc-system.drv".to_string(),
                outputs: vec![CveDerivationOutput {
                    name: "out".to_string(),
                    store_path: "/nix/store/cccccccccccccccccccccccccccccccc-system".to_string(),
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
                timeout_seconds: 60,
                scanner_args: vec!["--json".to_string()],
            },
            cache_source: None,
            lease_expires_at: Utc::now() + ChronoDuration::minutes(2),
        }
    }

    #[test]
    fn parser_accepts_exit_zero_and_vulnerability_exit_two() {
        assert_eq!(
            parse_vulnix_output(Some(0), b"[]", false)
                .expect("exit zero JSON")
                .len(),
            0
        );
        assert_eq!(
            parse_vulnix_output(Some(2), &vulnix_json(), false)
                .expect("exit two vulnerability JSON")
                .len(),
            1
        );
    }

    #[test]
    fn parser_rejects_malformed_and_oversized_output() {
        assert_eq!(
            parse_vulnix_output(Some(0), b"not-json", false)
                .expect_err("malformed JSON")
                .class,
            CveScanFailureClass::Deterministic
        );
        assert!(
            parse_vulnix_output(Some(0), b"[]", true)
                .expect_err("oversized output")
                .message
                .contains("exceeded")
        );
    }

    #[test]
    fn derivation_output_normalizer_accepts_v4_and_sorts_outputs() {
        let drv_path = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv";
        let normalized = normalize_derivation_show_output(
            br#"{
                "version": 4,
                "derivations": {
                  "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv": {
                    "version": 4,
                    "outputs": {
                        "dev": {"path": "cccccccccccccccccccccccccccccccc-package-dev"},
                        "out": {"path": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-package"}
                    }
                  }
                }
            }"#,
            &[drv_path.to_string()],
        )
        .expect("canonical version 4 derivation output");

        assert_eq!(
            normalized[drv_path],
            vec![
                ParsedDerivationOutput {
                    name: "dev".to_string(),
                    store_path: Some(
                        "/nix/store/cccccccccccccccccccccccccccccccc-package-dev".to_string(),
                    ),
                },
                ParsedDerivationOutput {
                    name: "out".to_string(),
                    store_path: Some(
                        "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-package".to_string(),
                    ),
                },
            ]
        );
    }

    #[test]
    fn derivation_output_normalizer_accepts_exact_multiple_derivation_set() {
        let first = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv";
        let second = "/nix/store/dddddddddddddddddddddddddddddddd-extra.drv";
        let normalized = normalize_derivation_show_output(
            br#"{
                "version": 4,
                "derivations": {
                    "dddddddddddddddddddddddddddddddd-extra.drv": {
                        "version": 4,
                        "outputs": {"out": {"path": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee-extra"}}
                    },
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv": {
                        "version": 4,
                        "outputs": {"out": {"path": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-package"}}
                    }
                }
            }"#,
            &[second.to_string(), first.to_string()],
        )
        .expect("exact multi-derivation response");

        assert_eq!(normalized.len(), 2);
        assert!(normalized.contains_key(first));
        assert!(normalized.contains_key(second));
    }

    #[test]
    fn derivation_output_normalizer_requires_exact_requested_set() {
        let requested = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv";
        let error = normalize_derivation_show_output(
            br#"{
                "version": 4,
                "derivations": {
                    "dddddddddddddddddddddddddddddddd-package.drv": {
                        "version": 4,
                        "outputs": {"out": {"path": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-package"}}
                    }
                }
            }"#,
            &[requested.to_string()],
        )
        .expect_err("a different derivation must not satisfy the requested set");

        assert_eq!(
            error.message,
            "Nix returned a mismatched derivation set (missing=1, extra=1, entries=1, requested=1)"
        );
        assert!(!error.message.contains("package.drv"));
        assert!(!error.message.contains("/nix/store/"));
    }

    #[test]
    fn derivation_output_normalizer_rejects_extra_derivation() {
        let requested = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv";
        let error = normalize_derivation_show_output(
            br#"{
                "version": 4,
                "derivations": {
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv": {
                        "version": 4,
                        "outputs": {"out": {"path": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-package"}}
                    },
                    "dddddddddddddddddddddddddddddddd-extra.drv": {
                        "version": 4,
                        "outputs": {"out": {"path": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee-extra"}}
                    }
                }
            }"#,
            &[requested.to_string()],
        )
        .expect_err("an extra derivation key must fail");

        assert_eq!(
            error.message,
            "Nix returned a mismatched derivation set (missing=0, extra=1, entries=2, requested=1)"
        );
        assert!(!error.message.contains("extra.drv"));
        assert!(!error.message.contains("/nix/store/"));
    }

    #[test]
    fn derivation_output_normalizer_rejects_malformed_shape_and_version() {
        let requested = vec!["/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv".to_string()];
        let malformed = normalize_derivation_show_output(b"not-json", &requested)
            .expect_err("malformed JSON must fail");
        assert!(malformed.message.contains("malformed JSON"));

        for unsupported in [
            br#"[]"#.as_slice(),
            br#"{"result":{"/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv":{"outputs":{}}}}"#.as_slice(),
            br#"{"version":4,"derivations":[]}"#.as_slice(),
        ] {
            let error = normalize_derivation_show_output(unsupported, &requested)
                .expect_err("unsupported derivation shape must fail");
            assert!(error.message.contains("malformed JSON"));
        }

        for unsupported_version in [
            br#"{"version":3,"derivations":{}}"#.as_slice(),
            br#"{"version":4,"derivations":{"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv":{"version":3,"outputs":{"out":{"path":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-package"}}}}}"#.as_slice(),
        ] {
            let error = normalize_derivation_show_output(unsupported_version, &requested)
                .expect_err("unsupported version must fail");
            assert!(error.message.contains("unsupported version"));
        }
    }

    #[test]
    fn derivation_output_normalizer_preserves_pathless_outputs_for_resolution() {
        let requested = vec!["/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv".to_string()];
        let normalized = normalize_derivation_show_output(
            br#"{"version":4,"derivations":{"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv":{"version":4,"outputs":{"out":{"hash":"sha256-example","method":"flat"}}}}}"#,
            &requested,
        )
        .expect("a legitimate pathless output must be deferred");
        assert_eq!(normalized[&requested[0]][0].store_path, None);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn pathless_derivation_output_resolves_by_exact_binding() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().expect("binding fixture directory");
        let script = directory.path().join("nix-store");
        std::fs::write(
            &script,
            "#!/bin/sh\n\
             test \"$1\" = --query || exit 11\n\
             test \"$2\" = --binding || exit 12\n\
             test \"$3\" = out || exit 13\n\
             test \"$4\" = /nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv || exit 14\n\
             printf '%s\\n' /nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-package\n",
        )
        .expect("binding fixture script");
        let mut permissions = std::fs::metadata(&script)
            .expect("binding fixture metadata")
            .permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).expect("binding fixture permissions");

        let drv_path = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv";
        let resolved = resolve_missing_derivation_outputs(
            &AcceptingApi,
            &claim(),
            BTreeMap::from([(
                drv_path.to_string(),
                vec![ParsedDerivationOutput {
                    name: "out".to_string(),
                    store_path: None,
                }],
            )]),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
            script.to_str().expect("UTF-8 fixture path"),
            Instant::now() + NIX_QUERY_TIMEOUT,
        )
        .await
        .expect("exact output binding");

        assert_eq!(
            resolved[drv_path],
            vec![CveDerivationOutput {
                name: "out".to_string(),
                store_path: "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-package".to_string(),
            }]
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn pathless_derivation_outputs_share_one_resolution_deadline() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().expect("binding timeout fixture directory");
        let script = directory.path().join("nix-store");
        std::fs::write(
            &script,
            "#!/bin/sh\nsleep 0.15\nprintf '%s\\n' /nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-package\n",
        )
        .expect("binding timeout fixture script");
        let mut permissions = std::fs::metadata(&script)
            .expect("binding timeout fixture metadata")
            .permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions)
            .expect("binding timeout fixture permissions");

        let parsed = || {
            BTreeMap::from([(
                "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv".to_string(),
                vec![ParsedDerivationOutput {
                    name: "out".to_string(),
                    store_path: None,
                }],
            )])
        };
        let resolution_deadline = Instant::now() + Duration::from_millis(250);
        resolve_missing_derivation_outputs(
            &AcceptingApi,
            &claim(),
            parsed(),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
            script.to_str().expect("UTF-8 fixture path"),
            resolution_deadline,
        )
        .await
        .expect("the first simulated chunk should fit the shared deadline");
        let error = resolve_missing_derivation_outputs(
            &AcceptingApi,
            &claim(),
            parsed(),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
            script.to_str().expect("UTF-8 fixture path"),
            resolution_deadline,
        )
        .await
        .expect_err("a later simulated chunk must use the remaining shared budget");

        assert_eq!(error.class, CveScanFailureClass::Transient);
        assert!(error.message.contains("timed out"));
    }

    #[test]
    fn pathless_output_limit_accumulates_across_chunks() {
        let parsed = |count| {
            BTreeMap::from([(
                "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv".to_string(),
                (0..count)
                    .map(|index| ParsedDerivationOutput {
                        name: format!("output-{index}"),
                        store_path: None,
                    })
                    .collect(),
            )])
        };
        let first_count = add_pathless_output_count(0, &parsed(128))
            .expect("the first chunk should fit the resolution limit");
        let error = add_pathless_output_count(first_count, &parsed(129))
            .expect_err("the cumulative pathless-output count must remain bounded");

        assert_eq!(error.class, CveScanFailureClass::Deterministic);
        assert!(error.message.contains("pathless-output limit"));
    }

    #[test]
    fn derivation_output_normalizer_rejects_empty_or_malformed_outputs() {
        let requested = vec!["/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv".to_string()];
        let empty = normalize_derivation_show_output(
            br#"{"version":4,"derivations":{"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv":{"version":4,"outputs":{}}}}"#,
            &requested,
        )
        .expect_err("an empty output set must fail");
        assert!(empty.message.contains("no outputs"));

        let null_path = normalize_derivation_show_output(
            br#"{"version":4,"derivations":{"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv":{"version":4,"outputs":{"out":{"path":null}}}}}"#,
            &requested,
        )
        .expect_err("an explicit null path must be malformed");
        assert!(null_path.message.contains("malformed output path"));
    }

    #[test]
    fn derivation_output_normalizer_rejects_non_basename_output_path() {
        let requested = vec!["/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv".to_string()];
        let error = normalize_derivation_show_output(
            br#"{"version":4,"derivations":{"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-package.drv":{"version":4,"outputs":{"out":{"path":"/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-package"}}}}}"#,
            &requested,
        )
        .expect_err("a version 4 output path must be a base name");

        assert!(error.message.contains("invalid store-path base name"));
    }

    #[test]
    fn api_status_mapping_preserves_revocation_and_failure_classes() {
        assert_eq!(
            super::super::api_client::cve_api_error_for_status(reqwest::StatusCode::GONE),
            CveApiError::Revoked
        );
        assert_eq!(
            super::super::api_client::cve_api_error_for_status(reqwest::StatusCode::FORBIDDEN)
                .failure_class(),
            CveScanFailureClass::Authorization
        );
        assert_eq!(
            super::super::api_client::cve_api_error_for_status(
                reqwest::StatusCode::SERVICE_UNAVAILABLE
            )
            .failure_class(),
            CveScanFailureClass::Transient
        );
    }

    #[test]
    fn canonical_result_orders_evidence_hashes_stably_and_preserves_whitelist() {
        let first_drv = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-alpha.drv";
        let second_drv = "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-beta.drv";
        let entries = vec![
            VulnixEntry {
                name: "beta-2".to_string(),
                pname: "beta".to_string(),
                version: "2".to_string(),
                affected_by: vec!["cve-2026-0002".to_string()],
                whitelisted: vec![],
                derivation: second_drv.to_string(),
                cvssv3_basescore: BTreeMap::from([("CVE-2026-0002".to_string(), 9.1)]),
            },
            VulnixEntry {
                name: "alpha-1".to_string(),
                pname: "alpha".to_string(),
                version: "1".to_string(),
                affected_by: vec![],
                whitelisted: vec![" CVE-2026-0001 ".to_string()],
                derivation: first_drv.to_string(),
                cvssv3_basescore: BTreeMap::new(),
            },
        ];
        let outputs = BTreeMap::from([
            (
                first_drv.to_string(),
                vec![CveDerivationOutput {
                    name: "out".to_string(),
                    store_path: "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-alpha".to_string(),
                }],
            ),
            (
                second_drv.to_string(),
                vec![CveDerivationOutput {
                    name: "out".to_string(),
                    store_path: "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-beta".to_string(),
                }],
            ),
        ]);
        let result = assemble_result(&claim(), entries, outputs).expect("canonical result");

        assert_eq!(result.entries[0].drv_path, first_drv);
        assert_eq!(result.entries[0].entry_id, 0);
        assert_eq!(result.entries[1].entry_id, 1);
        assert_eq!(result.observations[0].cve_id, "CVE-2026-0001");
        assert!(!result.observations[0].affected);
        assert!(result.observations[0].whitelisted);
        assert!(result.observations[1].affected);
        assert!(!result.observations[1].whitelisted);
        let digest = canonical_cve_result_digest(&result).expect("canonical result digest");
        assert_eq!(
            digest,
            "47f22397b0a170c162d23a0a44e3d5d86cbb51398b4ca89bdf46ee523ac8a92f"
        );
    }

    #[test]
    fn serialized_scan_reports_never_contain_diagnostic_credentials() {
        let diagnostic = scan_diagnostic(
            "error",
            "vulnix",
            "output",
            "Authorization: Bearer remote-secret\nhttps://user:pass@example.test/repo?token=query-secret password=hunter2 safe-context",
            false,
        );
        let claim = claim();
        let result = CveScanResult {
            schema_version: CveScanSchemaVersion::V1,
            scanner: claim.scanner.clone(),
            derivation: claim.derivation.clone(),
            entries: Vec::new(),
            observations: Vec::new(),
        };
        let complete = CveScanCompleteRequest {
            lease: claim.lease,
            result,
            result_digest_sha256: "0".repeat(64),
            scan_duration_ms: 1,
            diagnostics: vec![diagnostic.clone()],
        };
        let failed = CveScanFailRequest {
            lease: claim.lease,
            failure_class: CveScanFailureClass::Transient,
            error_message: "credential-free summary".to_string(),
            diagnostics: vec![diagnostic],
        };

        for encoded in [
            serde_json::to_string(&complete).expect("completion request should serialize"),
            serde_json::to_string(&failed).expect("failure request should serialize"),
        ] {
            assert!(encoded.contains("[REDACTED]"));
            assert!(encoded.contains("safe-context"));
            for secret in ["remote-secret", "user:pass", "query-secret", "hunter2"] {
                assert!(
                    !encoded.contains(secret),
                    "serialized request leaked {secret}"
                );
            }
        }
    }

    struct AcceptingApi;

    #[async_trait]
    impl CveLeaseApi for AcceptingApi {
        async fn heartbeat(&self, _request: &CveScanHeartbeatRequest) -> Result<bool, CveApiError> {
            Ok(true)
        }

        async fn complete(&self, _request: &CveScanCompleteRequest) -> Result<(), CveApiError> {
            Ok(())
        }

        async fn fail(&self, _request: &CveScanFailRequest) -> Result<(), CveApiError> {
            Ok(())
        }
    }

    struct RevokingApi;

    #[async_trait]
    impl CveLeaseApi for RevokingApi {
        async fn heartbeat(&self, _request: &CveScanHeartbeatRequest) -> Result<bool, CveApiError> {
            Ok(false)
        }

        async fn complete(&self, _request: &CveScanCompleteRequest) -> Result<(), CveApiError> {
            unreachable!("revoked work must not complete")
        }

        async fn fail(&self, _request: &CveScanFailRequest) -> Result<(), CveApiError> {
            Ok(())
        }
    }

    struct DelayedRevokingApi;

    #[async_trait]
    impl CveLeaseApi for DelayedRevokingApi {
        async fn heartbeat(&self, _request: &CveScanHeartbeatRequest) -> Result<bool, CveApiError> {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(false)
        }

        async fn complete(&self, _request: &CveScanCompleteRequest) -> Result<(), CveApiError> {
            unreachable!("revoked work must not complete")
        }

        async fn fail(&self, _request: &CveScanFailRequest) -> Result<(), CveApiError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn timeout_and_revocation_are_classified_and_stop_children() {
        let timeout = run_probe(
            "sh",
            &[
                "-c".to_string(),
                "printf 'timeout-secret' >&2; sleep 1".to_string(),
            ],
            Duration::from_millis(50),
            64,
        )
        .await
        .expect_err("sleep must time out");
        assert_eq!(timeout.class, CveScanFailureClass::Transient);
        assert!(
            timeout
                .diagnostics
                .iter()
                .any(|event| event.message.contains("timeout-secret")),
            "timeout diagnostics must retain stderr emitted before termination"
        );

        let revoked = run_leased_command(
            &RevokingApi,
            &claim(),
            "sleep",
            &["1".to_string()],
            Duration::from_secs(2),
            64,
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
        )
        .await
        .expect_err("first heartbeat must revoke the child");
        assert_eq!(revoked.class, CveScanFailureClass::Cancelled);
    }

    #[tokio::test]
    async fn exact_local_scanner_identity_must_match_claim() {
        let error = execute_claim_inner(
            &RevokingApi,
            &claim(),
            &CveScannerIdentity {
                name: "vulnix".to_string(),
                version: "1.12.1".to_string(),
            },
        )
        .await
        .expect_err("a different local version must not execute the claim");
        assert_eq!(error.class, CveScanFailureClass::Deterministic);
        assert!(error.message.contains("identity"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn heartbeat_revocation_kills_descendant_process_group() {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;

        let directory = tempfile::tempdir().expect("revocation fixture directory");
        let pid_file = directory.path().join("descendant.pid");

        let error = run_leased_command(
            &DelayedRevokingApi,
            &claim(),
            "sh",
            &[
                "-c".to_string(),
                "sleep 60 & echo $! > \"$1\"; wait".to_string(),
                "spawn-descendant".to_string(),
                pid_file.to_string_lossy().into_owned(),
            ],
            Duration::from_secs(2),
            64,
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
        )
        .await
        .expect_err("heartbeat must revoke the process group");
        assert_eq!(error.class, CveScanFailureClass::Cancelled);
        let descendant: i32 = tokio::fs::read_to_string(&pid_file)
            .await
            .expect("descendant PID should be published before revocation")
            .trim()
            .parse()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while kill(Pid::from_raw(descendant), None).is_ok() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("revoked descendant must terminate");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn timeout_and_future_drop_kill_descendant_process_groups() {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        use std::os::unix::fs::PermissionsExt;

        async fn wait_for_pid(path: &std::path::Path) -> i32 {
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    if let Ok(value) = tokio::fs::read_to_string(path).await
                        && let Ok(pid) = value.trim().parse()
                    {
                        return pid;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("descendant PID should be published")
        }

        async fn wait_for_exit(pid: i32) {
            tokio::time::timeout(Duration::from_secs(2), async {
                while kill(Pid::from_raw(pid), None).is_ok() {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("descendant process must terminate");
        }

        let directory = tempfile::tempdir().expect("process-group fixture directory");
        let script = directory.path().join("spawn-descendant");
        std::fs::write(&script, "#!/bin/sh\nsleep 60 &\necho $! > \"$1\"\nwait\n")
            .expect("fixture script");
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).unwrap();

        let timeout_pid_file = directory.path().join("timeout.pid");
        let timeout_error = run_probe(
            script.to_str().unwrap(),
            &[timeout_pid_file.to_string_lossy().into_owned()],
            Duration::from_millis(100),
            64,
        )
        .await
        .expect_err("fixture must time out");
        assert_eq!(timeout_error.class, CveScanFailureClass::Transient);
        wait_for_exit(wait_for_pid(&timeout_pid_file).await).await;

        let detached_script = directory.path().join("spawn-detached-descendant");
        std::fs::write(
            &detached_script,
            "#!/bin/sh\nsleep 60 &\necho $! > \"$1\"\nexit 0\n",
        )
        .expect("detached fixture script");
        let mut permissions = std::fs::metadata(&detached_script).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&detached_script, permissions).unwrap();
        let drain_pid_file = directory.path().join("drain-timeout.pid");
        let drain_error = run_probe(
            detached_script.to_str().unwrap(),
            &[drain_pid_file.to_string_lossy().into_owned()],
            Duration::from_millis(100),
            64,
        )
        .await
        .expect_err("a descendant that retains output pipes must not outlive the deadline");
        assert_eq!(drain_error.class, CveScanFailureClass::Transient);
        wait_for_exit(wait_for_pid(&drain_pid_file).await).await;

        let dropped_pid_file = directory.path().join("dropped.pid");
        let script_path = script.to_string_lossy().into_owned();
        let dropped_path = dropped_pid_file.to_string_lossy().into_owned();
        let task = tokio::spawn(async move {
            run_probe(&script_path, &[dropped_path], Duration::from_secs(60), 64).await
        });
        let dropped_pid = wait_for_pid(&dropped_pid_file).await;
        task.abort();
        let _ = task.await;
        wait_for_exit(dropped_pid).await;
    }
}
