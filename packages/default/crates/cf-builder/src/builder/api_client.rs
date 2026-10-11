use anyhow::{Context, Result};
use base64::Engine;
use cf_config::config::BuilderConfig;
use cf_protocol::builder::{
    BuildFailureClass, BuildFailurePhase, BuildProgressRequest, BuilderCapabilities,
    CveScanClaimRequest, CveScanClaimResponse, CveScanCompleteRequest, CveScanCompleteResponse,
    CveScanFailRequest, CveScanFailResponse, CveScanFailureClass, CveScanHeartbeatRequest,
    CveScanHeartbeatResponse, EstablishBuilderSessionRequest, EstablishBuilderSessionResponse,
    EvaluatorFingerprint, NextJobConflictReason, NextJobConflictResponse, NextJobRequest,
    NextJobResponse, RemoteBuildExecutionStrategy, ReportMetricsRequest, ResolveBuilderIdRequest,
    ResolveBuilderIdResponse,
};
use chrono::Utc;
use ed25519_dalek::{Signature, Signer, SigningKey};
use futures::SinkExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tokio::io::AsyncWriteExt;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tracing::{debug, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendLogsOutcome {
    Appended,
    Rejected,
    TerminalJob,
}

/// Classifies a signed CVE lease API failure without retaining response data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CveApiError {
    /// The server revoked the lease or superseded the builder session.
    Revoked,
    /// Authentication or authorization failed.
    Authorization,
    /// The server rejected the request as malformed or conflicting.
    Rejected,
    /// Transport failure, overload, or server failure can be retried.
    Transient,
}

impl CveApiError {
    /// Maps the API failure to the scan failure class reported by the builder.
    pub fn failure_class(self) -> CveScanFailureClass {
        match self {
            Self::Revoked => CveScanFailureClass::Cancelled,
            Self::Authorization => CveScanFailureClass::Authorization,
            Self::Rejected => CveScanFailureClass::Deterministic,
            Self::Transient => CveScanFailureClass::Transient,
        }
    }
}

impl std::fmt::Display for CveApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Revoked => "CVE lease was revoked",
            Self::Authorization => "CVE lease authorization failed",
            Self::Rejected => "CVE lease request was rejected",
            Self::Transient => "CVE lease request failed transiently",
        })
    }
}

impl std::error::Error for CveApiError {}

/// Classifies canonical source download failures before extraction.
#[derive(Debug)]
pub enum SourceArtifactDownloadError {
    /// Signed artifact metadata or downloaded bytes do not match.
    Identity(String),
    /// HTTP or local I/O prevented the transfer from completing.
    Transport(anyhow::Error),
}

impl std::fmt::Display for SourceArtifactDownloadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Identity(message) => formatter.write_str(message),
            Self::Transport(error) => write!(formatter, "{error:#}"),
        }
    }
}

impl std::error::Error for SourceArtifactDownloadError {}

pub fn job_status_requests_cancellation(status: Option<&str>) -> bool {
    matches!(status, Some("cancelling" | "cancelled"))
}

fn append_logs_conflict_is_terminal(error_text: &str) -> bool {
    let lower = error_text.to_ascii_lowercase();
    lower.contains("terminal")
        || lower.contains("cancelled")
        || lower.contains("success")
        || lower.contains("failed")
}

fn append_logs_outcome_for_status(
    status: reqwest::StatusCode,
    error_text: &str,
) -> AppendLogsOutcome {
    if status.is_success() {
        AppendLogsOutcome::Appended
    } else if status == reqwest::StatusCode::CONFLICT
        && append_logs_conflict_is_terminal(error_text)
    {
        AppendLogsOutcome::TerminalJob
    } else {
        AppendLogsOutcome::Rejected
    }
}

const DEFAULT_API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
// The lightweight server metadata probe has a 60-second budget. The extra
// 30 seconds cover persistence and transit, not another build or upload.
const BUILD_COMPLETION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);
const COMPLETION_REPORT_BUDGET: std::time::Duration = std::time::Duration::from_secs(600);
const COMPLETION_REPORT_MAX_ATTEMPTS: u32 = 5;
// Separate signed requests by at least two seconds to avoid timestamp replay.
const COMPLETION_REPORT_MIN_BACKOFF: std::time::Duration = std::time::Duration::from_secs(2);

/// Identifies a publication failure without retaining server text or URLs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationVerificationCode {
    /// The cache has not exposed the output path yet.
    CachePathMissing,
    /// The configured read credentials were rejected.
    CacheAuthenticationFailed,
    /// The cache rate-limited its metadata probe.
    CacheRateLimited,
    /// Network access to the cache failed.
    CacheNetworkError,
    /// The metadata probe exceeded its deadline.
    CacheProbeTimeout,
    /// The server cannot use the configured cache read settings.
    CacheReadConfigurationInvalid,
    /// The probe failed without a known transient classification.
    CacheProbeFailed,
}

impl PublicationVerificationCode {
    fn permits_retry(self) -> bool {
        matches!(
            self,
            Self::CachePathMissing
                | Self::CacheRateLimited
                | Self::CacheNetworkError
                | Self::CacheProbeTimeout
        )
    }
}

/// Records server acknowledgement independently from cache publication.
///
/// Pending publication is a durable successful build. Callers must not report
/// a build failure, rebuild, or upload again because publication is pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildCompletionReceipt {
    /// The server confirmed publication of the reported output.
    PublicationConfirmed,
    /// The server accepted completion without required publication.
    PublicationNotRequired,
    /// An older server accepted completion without publication status evidence.
    LegacyAccepted,
    /// The server completed the build but publication remains unconfirmed.
    PublicationPending {
        /// The last acknowledged, allowlisted verification failure.
        code: PublicationVerificationCode,
        /// The number of completion requests attempted in this call.
        attempts: u32,
    },
}

impl BuildCompletionReceipt {
    /// Returns whether an immediate post-build scanner claim is useful.
    ///
    /// Cache-backed builds require explicit publication confirmation. Pending
    /// and legacy acknowledgements leave scan selection to the server.
    pub fn permits_post_build_scan(self, publication_requested: bool) -> bool {
        !publication_requested || matches!(self, Self::PublicationConfirmed)
    }
}

#[derive(Deserialize)]
struct CompletionResponse {
    build_status: Option<String>,
    status: Option<String>,
    publication_status: Option<String>,
    verification_code: Option<PublicationVerificationCode>,
    retryable: Option<bool>,
    retry_after_secs: Option<u64>,
}

enum CompletionAttempt {
    Accepted(BuildCompletionReceipt),
    Pending(PublicationVerificationCode, Option<std::time::Duration>),
    Transient,
}

fn parse_completion_response(
    status: reqwest::StatusCode,
    body: &[u8],
) -> Result<CompletionAttempt> {
    if status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
    {
        return Ok(CompletionAttempt::Transient);
    }
    if status == reqwest::StatusCode::OK && body.is_empty() {
        return Ok(CompletionAttempt::Accepted(
            BuildCompletionReceipt::LegacyAccepted,
        ));
    }
    if !matches!(
        status,
        reqwest::StatusCode::OK | reqwest::StatusCode::ACCEPTED
    ) {
        anyhow::bail!("Completion report rejected (HTTP {})", status.as_u16());
    }
    // SECURITY: Never retain untrusted response bodies, parser errors, URLs, or
    // credentials in errors or tracing. Only typed allowlisted codes escape.
    let response: CompletionResponse = serde_json::from_slice(body)
        .map_err(|_| anyhow::anyhow!("Invalid completion acknowledgement"))?;
    let build_status = response
        .build_status
        .as_deref()
        .or(response.status.as_deref());
    if build_status != Some("success") {
        anyhow::bail!("Invalid completion build status");
    }
    match (status, response.publication_status.as_deref()) {
        (reqwest::StatusCode::OK, Some("confirmed")) => Ok(CompletionAttempt::Accepted(
            BuildCompletionReceipt::PublicationConfirmed,
        )),
        (reqwest::StatusCode::OK, Some("none")) => Ok(CompletionAttempt::Accepted(
            BuildCompletionReceipt::PublicationNotRequired,
        )),
        (reqwest::StatusCode::OK, None) => Ok(CompletionAttempt::Accepted(
            BuildCompletionReceipt::LegacyAccepted,
        )),
        (reqwest::StatusCode::ACCEPTED, Some("pending")) => {
            let code = response
                .verification_code
                .ok_or_else(|| anyhow::anyhow!("Missing publication verification code"))?;
            let retry = response.retryable == Some(true) && code.permits_retry();
            Ok(CompletionAttempt::Pending(
                code,
                retry.then(|| {
                    std::time::Duration::from_secs(response.retry_after_secs.unwrap_or(2))
                }),
            ))
        }
        _ => anyhow::bail!("Invalid completion publication status"),
    }
}
const DERIVATION_ARCHIVE_DOWNLOAD_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(30 * 60);

pub(crate) fn cve_api_error_for_status(status: reqwest::StatusCode) -> CveApiError {
    match status {
        reqwest::StatusCode::GONE => CveApiError::Revoked,
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            CveApiError::Authorization
        }
        reqwest::StatusCode::BAD_REQUEST
        | reqwest::StatusCode::NOT_FOUND
        | reqwest::StatusCode::CONFLICT
        | reqwest::StatusCode::UNPROCESSABLE_ENTITY => CveApiError::Rejected,
        _ => CveApiError::Transient,
    }
}

/// Error type for the delta derivation-transport endpoints that lets callers
/// distinguish "server doesn't support this yet" (fallback to full archive is
/// OK) from security/hard failures (fallback must NOT happen silently).
#[derive(Debug)]
pub enum DeltaError {
    /// Endpoint missing on this server (404/405). Full-archive fallback is safe.
    Unsupported(String),
    /// Auth, authorization, validation, transport, or import failure.
    /// Must not be masked by a silent fallback.
    Fatal(anyhow::Error),
}

impl std::fmt::Display for DeltaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeltaError::Unsupported(msg) => write!(f, "delta endpoint unsupported: {msg}"),
            DeltaError::Fatal(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for DeltaError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BuildStreamMessage {
    Log {
        message: String,
    },
    Metrics {
        cpu_percent: f32,
        ram_used_mb: u64,
        ram_total_mb: u64,
        timestamp: String,
    },
    /// Builder -> server: live build progress (WS primary path for the reporter).
    Progress {
        derivation_id: i32,
        elapsed_seconds: i32,
        current_target: Option<String>,
        last_activity_seconds: i32,
    },
    /// Server -> builder: operator requested cancellation.
    CancelRequested,
}

/// API client for builder-to-server communication
#[derive(Clone)]
pub struct BuilderApiClient {
    client: Client,
    server_url: String,
    builder_id: Uuid,
    builder_session_id: Uuid,
    signing_key: SigningKey,
    supported_execution_strategies: Vec<RemoteBuildExecutionStrategy>,
    supported_evaluator_contract_versions: Vec<u32>,
    evaluator: Option<EvaluatorFingerprint>,
    // The process advertisement is reused in every signed poll, independently
    // of CVE lease selection and optional scanner availability.
    capabilities: BuilderCapabilities,
}

impl BuilderApiClient {
    pub fn builder_id(&self) -> Uuid {
        self.builder_id
    }

    pub fn builder_session_id(&self) -> Uuid {
        self.builder_session_id
    }

    /// Create a new API client from configuration.
    ///
    /// If `builder_id` is not set in config, the builder ID is resolved
    /// dynamically from the server by signing a bootstrap request with the
    /// private key and calling `POST /api/v1/builders/resolve-id`.
    ///
    /// Resolution is retried with exponential backoff so that a builder whose
    /// public key has not yet been registered (or is currently disabled) does
    /// not crash the service or block a NixOS switch. Each failed attempt is
    /// logged with the builder's public key so an admin can register/enable it.
    /// The caller supplies the evaluator capability probed before polling, or
    /// `None` when this process cannot execute verified-source jobs. The caller
    /// also supplies capabilities after probing optional local executables.
    ///
    /// # Errors
    ///
    /// Returns an error when configuration, key loading, HTTP client creation,
    /// builder resolution, or session establishment fails.
    pub async fn new(
        config: &BuilderConfig,
        evaluator: Option<EvaluatorFingerprint>,
        capabilities: BuilderCapabilities,
    ) -> Result<Self> {
        let key_path = config.require_private_key_path()?;
        let server_url = config.require_server_url()?;

        // Load private key from file
        let signing_key =
            Self::load_private_key(&key_path).context("Failed to load builder private key")?;

        let client = Client::builder()
            .timeout(DEFAULT_API_TIMEOUT)
            .build()
            .context("Failed to create HTTP client")?;

        let builder_session_id = Uuid::new_v4();
        let builder_id = match config.builder_id {
            Some(builder_id) => {
                Self::establish_builder_session_with_retry(
                    &client,
                    &server_url,
                    &signing_key,
                    builder_id,
                    builder_session_id,
                    config.resolve_retry_interval,
                    config.resolve_retry_max_interval,
                    config.resolve_max_attempts,
                    capabilities.clone(),
                )
                .await?;
                builder_id
            }
            None => {
                Self::resolve_builder_id_with_retry(
                    &client,
                    &server_url,
                    &signing_key,
                    builder_session_id,
                    config.resolve_retry_interval,
                    config.resolve_retry_max_interval,
                    config.resolve_max_attempts,
                    capabilities.clone(),
                )
                .await?
            }
        };

        Ok(Self {
            client,
            server_url,
            builder_id,
            builder_session_id,
            signing_key,
            supported_execution_strategies: config.supported_execution_strategies.clone(),
            supported_evaluator_contract_versions: evaluator
                .as_ref()
                .map(|fingerprint| vec![fingerprint.contract_version])
                .unwrap_or_default(),
            evaluator,
            capabilities,
        })
    }

    async fn send_cve_request<TRequest, TResponse>(
        &self,
        operation: &str,
        request: &TRequest,
    ) -> std::result::Result<TResponse, CveApiError>
    where
        TRequest: Serialize + ?Sized,
        TResponse: for<'de> Deserialize<'de>,
    {
        let path = format!("/api/v1/builders/{}/cve-scans/{operation}", self.builder_id);
        let body = serde_json::to_vec(request).map_err(|_| CveApiError::Rejected)?;
        let (builder_id, signature, timestamp) = self.sign_request("POST", &path, &body);
        let response = self
            .client
            .post(format!("{}{}", self.server_url, path))
            .header("Content-Type", "application/json")
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .body(body)
            .send()
            .await
            .map_err(|_| CveApiError::Transient)?;
        let status = response.status();
        if !status.is_success() {
            return Err(cve_api_error_for_status(status));
        }
        response.json().await.map_err(|_| CveApiError::Rejected)
    }

    /// Claims one server-authorized CVE lease.
    ///
    /// # Errors
    ///
    /// Returns a classified error for transport, authorization, revocation,
    /// malformed response, or rejected request failures.
    pub async fn claim_cve_scan(
        &self,
        capabilities: BuilderCapabilities,
        completed_build_job_id: Option<Uuid>,
    ) -> std::result::Result<CveScanClaimResponse, CveApiError> {
        self.send_cve_request(
            "claim",
            &CveScanClaimRequest {
                builder_session_id: self.builder_session_id,
                capabilities,
                completed_build_job_id,
            },
        )
        .await
    }

    /// Renews one exact CVE execution lease.
    ///
    /// # Errors
    ///
    /// Returns a classified error for transport, authorization, revocation,
    /// malformed response, or rejected request failures.
    pub async fn heartbeat_cve_scan(
        &self,
        request: &CveScanHeartbeatRequest,
    ) -> std::result::Result<CveScanHeartbeatResponse, CveApiError> {
        self.send_cve_request("heartbeat", request).await
    }

    /// Completes one exact CVE execution lease with canonical evidence.
    ///
    /// # Errors
    ///
    /// Returns a classified error for transport, authorization, revocation,
    /// malformed response, or rejected evidence failures.
    pub async fn complete_cve_scan(
        &self,
        request: &CveScanCompleteRequest,
    ) -> std::result::Result<CveScanCompleteResponse, CveApiError> {
        self.send_cve_request("complete", request).await
    }

    /// Reports a scanner failure independently from its producing build.
    ///
    /// # Errors
    ///
    /// Returns a classified error for transport, authorization, revocation,
    /// malformed response, or rejected request failures.
    pub async fn fail_cve_scan(
        &self,
        request: &CveScanFailRequest,
    ) -> std::result::Result<CveScanFailResponse, CveApiError> {
        self.send_cve_request("fail", request).await
    }

    /// Resolve the builder ID, retrying with exponential backoff on failure.
    ///
    /// A failure here typically means the builder's public key is not yet
    /// registered on the server, or the builder is currently disabled. Rather
    /// than exiting (which would crash the service and fail a NixOS switch), we
    /// keep retrying and logging so an administrator has time to register the
    /// public key and enable the builder in the UI.
    async fn resolve_builder_id_with_retry(
        client: &Client,
        server_url: &str,
        signing_key: &SigningKey,
        builder_session_id: Uuid,
        retry_interval: std::time::Duration,
        max_interval: std::time::Duration,
        max_attempts: u32,
        capabilities: BuilderCapabilities,
    ) -> Result<Uuid> {
        let public_key = Self::public_key_base64_for(signing_key);
        let mut delay = retry_interval.max(std::time::Duration::from_secs(1));
        let mut attempt: u32 = 0;

        loop {
            attempt += 1;
            match Self::resolve_builder_id(
                client,
                server_url,
                signing_key,
                builder_session_id,
                capabilities.clone(),
            )
            .await
            {
                Ok(builder_id) => {
                    if attempt > 1 {
                        info!(
                            "✅ Builder ID resolved after {} attempt(s): {}",
                            attempt, builder_id
                        );
                    }
                    return Ok(builder_id);
                }
                Err(e) => {
                    if max_attempts != 0 && attempt >= max_attempts {
                        anyhow::bail!(
                            "Builder ID resolution failed after {} attempt(s): {}. \
                             Public key: {}",
                            attempt,
                            e,
                            public_key
                        );
                    }

                    warn!(
                        "⏳ Builder not ready yet (attempt {}): {}. \
                         Register this builder's public key in Crystal Forge and ensure it is enabled. \
                         Public key (base64): {}. Retrying in {:?}.",
                        attempt, e, public_key, delay
                    );

                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(delay.saturating_mul(2), max_interval);
                }
            }
        }
    }

    /// Load Ed25519 private key from file
    fn load_private_key(path: &Path) -> Result<SigningKey> {
        // Read the key file as string (base64 encoded, like agent keys)
        let key_string = fs::read_to_string(path)
            .context("Failed to read private key file")?
            .trim()
            .to_string();

        // Decode base64 to raw bytes
        let key_data = base64::engine::general_purpose::STANDARD
            .decode(&key_string)
            .context("Failed to decode base64 private key")?;

        if key_data.len() != 32 {
            anyhow::bail!(
                "Invalid private key: expected 32 bytes after base64 decode, got {}",
                key_data.len()
            );
        }

        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&key_data);

        Ok(SigningKey::from_bytes(&key_bytes))
    }

    fn canonical_signature_payload(
        method: &str,
        path: &str,
        timestamp: &str,
        body: &[u8],
    ) -> Vec<u8> {
        let mut payload =
            Vec::with_capacity(method.len() + path.len() + timestamp.len() + body.len() + 3);
        payload.extend_from_slice(method.as_bytes());
        payload.push(b'\n');
        payload.extend_from_slice(path.as_bytes());
        payload.push(b'\n');
        payload.extend_from_slice(timestamp.as_bytes());
        payload.push(b'\n');
        payload.extend_from_slice(body);
        payload
    }

    /// Sign canonical builder API payload and return auth headers values.
    fn sign_request(&self, method: &str, path: &str, body: &[u8]) -> (String, String, String) {
        let timestamp = Utc::now().to_rfc3339();
        let payload = Self::canonical_signature_payload(method, path, &timestamp, body);
        let signature: Signature = self.signing_key.sign(&payload);

        (
            self.builder_id.to_string(),
            base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
            timestamp,
        )
    }

    async fn establish_builder_session_with_retry(
        client: &Client,
        server_url: &str,
        signing_key: &SigningKey,
        builder_id: Uuid,
        builder_session_id: Uuid,
        retry_interval: std::time::Duration,
        max_interval: std::time::Duration,
        max_attempts: u32,
        capabilities: BuilderCapabilities,
    ) -> Result<()> {
        let mut delay = retry_interval.max(std::time::Duration::from_secs(1));
        let mut attempt: u32 = 0;

        loop {
            attempt += 1;
            match Self::establish_builder_session(
                client,
                server_url,
                signing_key,
                builder_id,
                builder_session_id,
                capabilities.clone(),
            )
            .await
            {
                Ok(()) => return Ok(()),
                Err(e) => {
                    if max_attempts != 0 && attempt >= max_attempts {
                        anyhow::bail!(
                            "Builder session establishment failed after {} attempt(s): {}",
                            attempt,
                            e
                        );
                    }

                    warn!(
                        "⏳ Builder session not ready yet (attempt {}): {}. Retrying in {:?}.",
                        attempt, e, delay
                    );

                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(delay.saturating_mul(2), max_interval);
                }
            }
        }
    }

    async fn establish_builder_session(
        client: &Client,
        server_url: &str,
        signing_key: &SigningKey,
        builder_id: Uuid,
        builder_session_id: Uuid,
        capabilities: BuilderCapabilities,
    ) -> Result<()> {
        let path = format!("/api/v1/builders/{}/session", builder_id);
        let url = format!("{}{}", server_url, path);
        let body = serde_json::to_vec(&EstablishBuilderSessionRequest {
            session_id: builder_session_id,
            capabilities,
        })?;
        let (signature, timestamp) =
            Self::sign_bootstrap_request(signing_key, "POST", &path, &body);

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Builder-ID", builder_id.to_string())
            .header("X-Builder-Session-ID", builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .body(body)
            .send()
            .await
            .context("Failed to establish builder session")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!(
                "Builder session establishment failed with status {}: {}",
                status,
                error_text
            );
        }

        let established: EstablishBuilderSessionResponse = response
            .json()
            .await
            .context("Failed to decode builder session response")?;
        if established.session_id != builder_session_id {
            anyhow::bail!("server returned mismatched builder session ID");
        }

        Ok(())
    }

    /// Return the builder public key (base64) derived from configured private key.
    pub fn public_key_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD
            .encode(self.signing_key.verifying_key().to_bytes())
    }

    fn public_key_base64_for(signing_key: &SigningKey) -> String {
        base64::engine::general_purpose::STANDARD.encode(signing_key.verifying_key().to_bytes())
    }

    pub(crate) fn sign_bootstrap_request(
        signing_key: &SigningKey,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> (String, String) {
        let timestamp = Utc::now().to_rfc3339();
        let payload = Self::canonical_signature_payload(method, path, &timestamp, body);
        let signature: Signature = signing_key.sign(&payload);

        (
            base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
            timestamp,
        )
    }

    async fn resolve_builder_id(
        client: &Client,
        server_url: &str,
        signing_key: &SigningKey,
        builder_session_id: Uuid,
        capabilities: BuilderCapabilities,
    ) -> Result<Uuid> {
        let path = "/api/v1/builders/resolve-id";
        let url = format!("{}{}", server_url, path);
        let body = serde_json::to_vec(&ResolveBuilderIdRequest {
            public_key: Self::public_key_base64_for(signing_key),
            session_id: Some(builder_session_id),
            capabilities,
        })?;
        let (signature, timestamp) = Self::sign_bootstrap_request(signing_key, "POST", path, &body);

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .header("X-Builder-Session-ID", builder_session_id.to_string())
            .body(body)
            .send()
            .await
            .context("Failed to resolve builder ID")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!(
                "Builder ID resolution failed with status {}: {}. Register this builder's public key in Crystal Forge and ensure it is enabled.",
                status,
                error_text
            );
        }

        let resolved: ResolveBuilderIdResponse = response
            .json()
            .await
            .context("Failed to decode builder ID resolution response")?;
        if resolved.session_id != Some(builder_session_id) {
            anyhow::bail!("server returned mismatched builder session ID");
        }
        Ok(resolved.builder_id)
    }

    /// Send heartbeat and metrics to server
    pub async fn send_heartbeat(&self, metrics: &ReportMetricsRequest) -> Result<()> {
        let path = format!("/api/v1/builders/{}/heartbeat", self.builder_id);
        let url = format!("{}{}", self.server_url, path);
        let body = serde_json::to_vec(metrics)?;
        let (builder_id, signature, timestamp) = self.sign_request("POST", &path, &body);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .body(body)
            .send()
            .await
            .context("Failed to send heartbeat")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!("Heartbeat failed with status {}: {}", status, error_text);
        }

        debug!("Heartbeat sent successfully");
        Ok(())
    }

    /// Get the next available job from the server, including the embedded
    /// derivation build payload so the builder needs no database access.
    /// Sends the process capabilities in the signed POST. An unsupported cache
    /// conflict returns no work; the server leaves the candidate unclaimed.
    ///
    /// # Errors
    /// Returns an error for serialization or transport failures, invalid job
    /// responses, superseded sessions, or other server rejection statuses.
    pub async fn get_next_job(&self) -> Result<Option<NextJobResponse>> {
        let body = serde_json::to_vec(&NextJobRequest {
            capabilities: self.capabilities.clone(),
            protocol_version: 2,
            supported_execution_strategies: self.supported_execution_strategies.clone(),
            supported_evaluator_contract_versions: self
                .supported_evaluator_contract_versions
                .clone(),
            evaluator: self.evaluator.clone(),
        })?;

        let response = self.send_next_job_request("POST", body).await?;
        if response.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            if !self
                .supported_execution_strategies
                .contains(&RemoteBuildExecutionStrategy::ServerDerivation)
            {
                anyhow::bail!(
                    "server only supports legacy GET polling, but this builder does not support server_derivation"
                );
            }

            warn!(
                "⚠️  Server rejected POST /next-job with 405; retrying legacy GET for rolling upgrade compatibility"
            );
            return self
                .parse_next_job_response(self.send_next_job_request("GET", Vec::new()).await?)
                .await;
        }

        self.parse_next_job_response(response).await
    }

    async fn send_next_job_request(
        &self,
        method: &str,
        body: Vec<u8>,
    ) -> Result<reqwest::Response> {
        let path = format!("/api/v1/builders/{}/next-job", self.builder_id);
        let url = format!("{}{}", self.server_url, path);
        let (builder_id, signature, timestamp) = self.sign_request(method, &path, &body);

        let request = match method {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            _ => anyhow::bail!("unsupported next-job request method {method}"),
        };

        request
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .context("Failed to request next job")
    }

    async fn parse_next_job_response(
        &self,
        response: reqwest::Response,
    ) -> Result<Option<NextJobResponse>> {
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            // No jobs available
            return Ok(None);
        }

        if response.status() == reqwest::StatusCode::GONE {
            anyhow::bail!(
                "Builder session {} has been superseded (410 Gone). \
                 Re-establish identity to obtain a new session.",
                self.builder_session_id,
            );
        }

        if response.status() == reqwest::StatusCode::CONFLICT {
            let body = response.bytes().await.unwrap_or_default();
            match serde_json::from_slice::<NextJobConflictResponse>(&body)
                .map(|response| response.reason)
            {
                Ok(NextJobConflictReason::UnsupportedCacheType) => warn!(
                    "Selected cache requires a newer builder capability (409 Conflict); no job was claimed"
                ),
                Ok(NextJobConflictReason::UnsupportedExecutionStrategy) => warn!(
                    supported_strategies = ?self.supported_execution_strategies,
                    "Server's configured remote execution strategy is not supported by this builder (409 Conflict)"
                ),
                Ok(NextJobConflictReason::IncompatibleEvaluator) => warn!(
                    builder_evaluator = ?self.evaluator,
                    "Builder evaluator fingerprint does not match the server's authoritative evaluator (409 Conflict)"
                ),
                Ok(NextJobConflictReason::IncompatibleSourceDelivery) => warn!(
                    "Server source delivery mode is incompatible with the verified-source evaluator contract (409 Conflict)"
                ),
                Ok(NextJobConflictReason::SourceMaterializationCancelled) => warn!(
                    "Server cancelled canonical source preparation before claim; the builder will poll again (409 Conflict)"
                ),
                Err(error) => warn!(
                    error = %error,
                    "Server rejected next-job polling with an unstructured or unknown 409 Conflict response"
                ),
            }
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!("Get next job failed with status {}: {}", status, error_text);
        }

        let next_job: NextJobResponse = response
            .json()
            .await
            .context("Failed to parse job response")?;

        Ok(Some(next_job))
    }

    /// Report build progress to the server over HTTP (reporter fallback path).
    pub async fn report_progress(
        &self,
        job_id: uuid::Uuid,
        progress: &BuildProgressRequest,
    ) -> Result<()> {
        let path = format!(
            "/api/v1/builders/{}/jobs/{}/progress",
            self.builder_id, job_id
        );
        let url = format!("{}{}", self.server_url, path);
        let body = serde_json::to_vec(progress)?;
        let (builder_id, signature, timestamp) = self.sign_request("POST", &path, &body);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .body(body)
            .send()
            .await
            .context("Failed to report build progress")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!(
                "Report progress failed with status {}: {}",
                status,
                error_text
            );
        }

        Ok(())
    }

    /// Download a Nix archive for the job's derivation closure from the server.
    pub async fn download_derivation_archive(&self, job_id: uuid::Uuid) -> Result<bytes::Bytes> {
        let path = format!(
            "/api/v1/builders/{}/jobs/{}/derivation-archive",
            self.builder_id, job_id
        );
        let url = format!("{}{}", self.server_url, path);
        let body = Vec::new();
        let (builder_id, signature, timestamp) = self.sign_request("GET", &path, &body);

        let response = self
            .client
            .get(&url)
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .timeout(DERIVATION_ARCHIVE_DOWNLOAD_TIMEOUT)
            .send()
            .await
            .context("Failed to request derivation archive")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!(
                "Download derivation archive failed with status {}: {}",
                status,
                error_text
            );
        }

        response
            .bytes()
            .await
            .context("Failed to read derivation archive response")
    }

    /// Stream the derivation archive directly into `nix-store --import` without
    /// buffering the entire closure in memory.
    ///
    /// This avoids the multi-GiB RAM spike that occurs when large NixOS closures
    /// are downloaded as a single blob before being imported.
    pub async fn stream_derivation_archive_to_import(
        &self,
        job_id: uuid::Uuid,
        drv_path: &str,
    ) -> Result<()> {
        let path = format!(
            "/api/v1/builders/{}/jobs/{}/derivation-archive",
            self.builder_id, job_id
        );
        let url = format!("{}{}", self.server_url, path);
        let body = Vec::new();
        let (builder_id, signature, timestamp) = self.sign_request("GET", &path, &body);

        let response = self
            .client
            .get(&url)
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .timeout(DERIVATION_ARCHIVE_DOWNLOAD_TIMEOUT)
            .send()
            .await
            .context("Failed to request derivation archive for streaming import")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!(
                "Derivation archive stream request failed with status {}: {}",
                status,
                error_text
            );
        }

        // Spawn nix-store --import and pipe the HTTP response stream to its stdin.
        let mut child = tokio::process::Command::new("nix-store")
            .arg("--import")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn nix-store --import")?;

        let mut stdin = child
            .stdin
            .take()
            .context("Failed to get nix-store --import stdin")?;

        // Stream response body chunks into nix-store --import stdin.
        let mut byte_stream = response.bytes_stream();
        use futures::StreamExt;
        while let Some(chunk) = byte_stream.next().await {
            let chunk = chunk.context("Error reading derivation archive chunk from server")?;
            stdin
                .write_all(&chunk)
                .await
                .context("Failed to write derivation archive chunk to nix-store --import")?;
        }
        // Close stdin so nix-store knows the stream is done.
        drop(stdin);

        let output = child
            .wait_with_output()
            .await
            .context("Failed to wait for nix-store --import")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("nix-store --import failed: {stderr}");
        }

        if !std::path::Path::new(drv_path).exists() {
            anyhow::bail!(
                "nix-store --import succeeded but derivation path is still missing: {drv_path}"
            );
        }

        Ok(())
    }

    /// Fetch the authorized requisite path manifest for a job's `.drv`.
    ///
    /// Returns `DeltaError::Unsupported` for 404/405 so the caller can fall
    /// back to the full archive endpoint on older servers. All other failures
    /// (including 403) are `DeltaError::Fatal` and must NOT be silently
    /// retried through the fallback path.
    pub async fn get_derivation_manifest(
        &self,
        job_id: uuid::Uuid,
    ) -> std::result::Result<cf_protocol::builder::DerivationManifestResponse, DeltaError> {
        let path = format!(
            "/api/v1/builders/{}/jobs/{}/derivation-manifest",
            self.builder_id, job_id
        );
        let url = format!("{}{}", self.server_url, path);
        let body = Vec::new();
        let (builder_id, signature, timestamp) = self.sign_request("GET", &path, &body);

        let response = self
            .client
            .get(&url)
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .timeout(DEFAULT_API_TIMEOUT)
            .send()
            .await
            .map_err(|e| DeltaError::Fatal(anyhow::anyhow!("manifest request failed: {e}")))?;

        match response.status() {
            s if s.is_success() => response
                .json::<cf_protocol::builder::DerivationManifestResponse>()
                .await
                .map_err(|e| {
                    DeltaError::Fatal(anyhow::anyhow!("malformed manifest response: {e}"))
                }),
            reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::METHOD_NOT_ALLOWED => {
                Err(DeltaError::Unsupported(format!(
                    "derivation-manifest endpoint returned {}",
                    response.status()
                )))
            }
            status => {
                let text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "unknown error".to_string());
                Err(DeltaError::Fatal(anyhow::anyhow!(
                    "manifest request failed with status {status}: {text}"
                )))
            }
        }
    }

    /// POST the missing path list to the delta archive endpoint and stream the
    /// response directly into `nix-store --import`.
    ///
    /// `204 No Content` is success only when `paths` is empty. 404/405 map to
    /// `DeltaError::Unsupported`; 403 and other errors are `DeltaError::Fatal`.
    pub async fn stream_derivation_delta_archive_to_import(
        &self,
        job_id: uuid::Uuid,
        paths: &[String],
    ) -> std::result::Result<(), DeltaError> {
        let path = format!(
            "/api/v1/builders/{}/jobs/{}/derivation-archive",
            self.builder_id, job_id
        );
        let url = format!("{}{}", self.server_url, path);
        let request = cf_protocol::builder::DerivationArchiveRequest {
            paths: paths.to_vec(),
        };
        let body = serde_json::to_vec(&request)
            .map_err(|e| DeltaError::Fatal(anyhow::anyhow!("failed to serialize request: {e}")))?;
        let (builder_id, signature, timestamp) = self.sign_request("POST", &path, &body);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .timeout(DERIVATION_ARCHIVE_DOWNLOAD_TIMEOUT)
            .body(body)
            .send()
            .await
            .map_err(|e| DeltaError::Fatal(anyhow::anyhow!("delta archive request failed: {e}")))?;

        match response.status() {
            reqwest::StatusCode::NO_CONTENT => {
                if paths.is_empty() {
                    Ok(())
                } else {
                    Err(DeltaError::Fatal(anyhow::anyhow!(
                        "server returned 204 No Content for a non-empty delta request ({} paths)",
                        paths.len()
                    )))
                }
            }
            s if s.is_success() => self
                .pipe_response_to_nix_import(response)
                .await
                .map_err(DeltaError::Fatal),
            reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::METHOD_NOT_ALLOWED => {
                Err(DeltaError::Unsupported(format!(
                    "delta derivation-archive endpoint returned {}",
                    response.status()
                )))
            }
            status => {
                let text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "unknown error".to_string());
                Err(DeltaError::Fatal(anyhow::anyhow!(
                    "delta archive request failed with status {status}: {text}"
                )))
            }
        }
    }

    /// Pipe an HTTP response body stream into `nix-store --import` without
    /// buffering the archive in memory.
    async fn pipe_response_to_nix_import(&self, response: reqwest::Response) -> Result<()> {
        let mut child = tokio::process::Command::new("nix-store")
            .arg("--import")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn nix-store --import")?;

        let mut stdin = child
            .stdin
            .take()
            .context("Failed to get nix-store --import stdin")?;

        let mut byte_stream = response.bytes_stream();
        use futures::StreamExt;
        while let Some(chunk) = byte_stream.next().await {
            let chunk = chunk.context("Error reading delta archive chunk from server")?;
            stdin
                .write_all(&chunk)
                .await
                .context("Failed to write delta archive chunk to nix-store --import")?;
        }
        drop(stdin);

        let output = child
            .wait_with_output()
            .await
            .context("Failed to wait for nix-store --import")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("nix-store --import failed: {stderr}");
        }

        Ok(())
    }

    /// Streams the canonical source artifact for a verified-source job.
    ///
    /// The method enforces the authorized artifact size and SHA-256 digest while it
    /// streams. It never buffers the complete artifact in memory.
    ///
    /// Returns the path of the downloaded temp file. Callers are responsible
    /// for extracting it and removing it afterwards.
    ///
    /// # Errors
    ///
    /// Returns [`SourceArtifactDownloadError::Identity`] when signed metadata
    /// or downloaded bytes violate the authorized identity. Returns
    /// [`SourceArtifactDownloadError::Transport`] for HTTP and local I/O
    /// failures that do not prove an identity mismatch.
    pub async fn stream_source_archive_to_tempfile(
        &self,
        job_id: uuid::Uuid,
        expected_sha256: &str,
        expected_size: u64,
        dest_dir: &std::path::Path,
    ) -> std::result::Result<std::path::PathBuf, SourceArtifactDownloadError> {
        use sha2::{Digest, Sha256};
        use tokio::io::AsyncWriteExt;

        let path = format!(
            "/api/v1/builders/{}/jobs/{}/source-archive",
            self.builder_id, job_id
        );
        let url = format!("{}{}", self.server_url, path);
        let body = Vec::new();
        let (builder_id, signature, timestamp) = self.sign_request("GET", &path, &body);

        let response = self
            .client
            .get(&url)
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .timeout(DERIVATION_ARCHIVE_DOWNLOAD_TIMEOUT)
            .send()
            .await
            .map_err(|error| {
                SourceArtifactDownloadError::Transport(
                    anyhow::Error::new(error).context("Failed to request source archive"),
                )
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(SourceArtifactDownloadError::Transport(anyhow::anyhow!(
                "Download source archive failed with status {status}: {error_text}"
            )));
        }
        if expected_size > cf_protocol::source_artifact::VERIFIED_SOURCE_ARTIFACT_MAX_BYTES {
            return Err(SourceArtifactDownloadError::Identity(
                "source artifact exceeds the protocol size limit".to_string(),
            ));
        }
        if response
            .content_length()
            .is_some_and(|size| size != expected_size)
        {
            return Err(SourceArtifactDownloadError::Identity(
                "source artifact Content-Length does not match the authorized manifest".to_string(),
            ));
        }

        // Stream to a temp file, computing SHA-256 on the fly.
        tokio::fs::create_dir_all(dest_dir).await.map_err(|error| {
            SourceArtifactDownloadError::Transport(
                anyhow::Error::new(error).context("Failed to create source archive temp directory"),
            )
        })?;
        let tmp_path = dest_dir.join(format!(
            "source-artifact-{job_id}-{}.tar.tmp",
            uuid::Uuid::new_v4()
        ));
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .await
            .map_err(|error| {
                SourceArtifactDownloadError::Transport(
                    anyhow::Error::new(error).context("Failed to create source artifact temp file"),
                )
            })?;

        let mut hasher = Sha256::new();
        let mut downloaded = 0_u64;
        let mut byte_stream = response.bytes_stream();
        use futures::StreamExt;
        while let Some(chunk) = byte_stream.next().await {
            let chunk = chunk.map_err(|error| {
                SourceArtifactDownloadError::Transport(
                    anyhow::Error::new(error)
                        .context("Error reading source archive chunk from server"),
                )
            })?;
            downloaded = downloaded.checked_add(chunk.len() as u64).ok_or_else(|| {
                SourceArtifactDownloadError::Identity("source artifact size overflowed".to_string())
            })?;
            if downloaded > expected_size {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(SourceArtifactDownloadError::Identity(
                    "source artifact exceeds the authorized manifest size".to_string(),
                ));
            }
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(|error| {
                SourceArtifactDownloadError::Transport(
                    anyhow::Error::new(error)
                        .context("Failed to write source archive chunk to temp file"),
                )
            })?;
        }
        file.flush().await.map_err(|error| {
            SourceArtifactDownloadError::Transport(
                anyhow::Error::new(error).context("Failed to flush source archive temp file"),
            )
        })?;
        drop(file);

        if downloaded != expected_size {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(SourceArtifactDownloadError::Identity(
                "source artifact is shorter than the authorized manifest size".to_string(),
            ));
        }
        let actual = format!("{:x}", hasher.finalize());
        if actual != expected_sha256 {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(SourceArtifactDownloadError::Identity(format!(
                "source artifact SHA-256 mismatch: expected {expected_sha256}, got {actual}"
            )));
        }
        info!("✅ Source artifact SHA-256 verified: {}", actual);

        Ok(tmp_path)
    }

    /// Ask the server to publish the job's derivation closure to the configured
    /// binary cache so the builder can fetch it through normal Nix substituters.
    pub async fn publish_derivation_closure(&self, job_id: uuid::Uuid) -> Result<()> {
        let path = format!(
            "/api/v1/builders/{}/jobs/{}/publish-derivation-closure",
            self.builder_id, job_id
        );
        let url = format!("{}{}", self.server_url, path);
        let body = Vec::new();
        let (builder_id, signature, timestamp) = self.sign_request("POST", &path, &body);

        let response = self
            .client
            .post(&url)
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .timeout(DERIVATION_ARCHIVE_DOWNLOAD_TIMEOUT)
            .send()
            .await
            .context("Failed to request derivation closure publish")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!(
                "Publish derivation closure failed with status {}: {}",
                status,
                error_text
            );
        }

        Ok(())
    }

    /// Start a job (mark it as in-progress)
    pub async fn start_job(&self, job_id: uuid::Uuid) -> Result<()> {
        let path = format!("/api/v1/builders/{}/jobs/{}/start", self.builder_id, job_id);
        let url = format!("{}{}", self.server_url, path);
        let body = Vec::new();
        let (builder_id, signature, timestamp) = self.sign_request("POST", &path, &body);

        let response = self
            .client
            .post(&url)
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .send()
            .await
            .context("Failed to start job")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!("Start job failed with status {}: {}", status, error_text);
        }

        info!("Job {} started", job_id);
        Ok(())
    }

    /// Reports successful build completion and the published destination identity.
    ///
    /// Callers must supply a cache reference and destination ID only after a
    /// successful push. Without a push, the destination ID is omitted even if
    /// the caller supplied one.
    /// Retries reporting only, with fresh request signatures and the same
    /// output, destination, and session. At most five requests run within ten
    /// minutes; each attempt has a 90-second deadline. Retry delays are at least
    /// two seconds. Pending publication retries require both a transient code
    /// and the server's retryable flag. A pending acknowledgement remains a
    /// successful durable build even if later reporting fails or times out.
    /// Dropping this future stops reporting; restart recovery is server-owned.
    /// Cache references remain opaque authorized payloads. Reporting preserves
    /// their exact bytes and never includes them in errors or tracing.
    ///
    /// # Errors
    ///
    /// Returns a credential-safe error for serialization, invalid responses,
    /// rejection, or exhausted transient reporting without a durable receipt.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use cf_builder::builder::api_client::BuilderApiClient;
    /// # async fn report(client: &BuilderApiClient, job_id: uuid::Uuid)
    /// #     -> anyhow::Result<()> {
    /// // The build and push already succeeded. Report their exact identity.
    /// let receipt = client.complete_job(
    ///     job_id, "/nix/store/output", Some("https://cache.example"), Some(470),
    /// ).await?;
    /// if receipt.permits_post_build_scan(true) {
    ///     // The server confirmed publication; a scan claim can now be useful.
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn complete_job(
        &self,
        job_id: uuid::Uuid,
        output_path: &str,
        cache_reference: Option<&str>,
        cache_destination_id: Option<i32>,
    ) -> Result<BuildCompletionReceipt> {
        let path = format!(
            "/api/v1/builders/{}/jobs/{}/complete",
            self.builder_id, job_id
        );
        let request = cf_protocol::builder::CompleteJobRequest {
            output_path: Some(output_path.to_string()),
            cache_pushed: cache_reference.is_some(),
            cache_reference: cache_reference.map(ToString::to_string),
            cache_destination_id: cache_reference.and(cache_destination_id),
        };
        let body = serde_json::to_vec(&request)
            .map_err(|_| anyhow::anyhow!("Cannot serialize completion report"))?;
        self.report_completion_request(
            &path,
            body,
            COMPLETION_REPORT_BUDGET,
            BUILD_COMPLETION_TIMEOUT,
        )
        .await
    }

    // Keep reporting separate from work execution. Explicit deadlines also let
    // tests exercise stalled responses without a real 90-second probe.
    async fn report_completion_request(
        &self,
        path: &str,
        body: Vec<u8>,
        budget: std::time::Duration,
        attempt_timeout: std::time::Duration,
    ) -> Result<BuildCompletionReceipt> {
        let url = format!("{}{}", self.server_url, path);
        let deadline = tokio::time::Instant::now() + budget;
        let mut pending_code = None;
        let mut attempts = 0;
        let mut last_error = anyhow::anyhow!("Completion reporting budget exhausted");
        while attempts < COMPLETION_REPORT_MAX_ATTEMPTS && tokio::time::Instant::now() < deadline {
            attempts += 1;
            let attempt_deadline = deadline.min(tokio::time::Instant::now() + attempt_timeout);
            let result = tokio::time::timeout_at(attempt_deadline, async {
                let (builder_id, signature, timestamp) = self.sign_request("POST", path, &body);
                let response = match self
                    .client
                    .post(&url)
                    .timeout(attempt_timeout)
                    .header("Content-Type", "application/json")
                    .header("X-Builder-ID", builder_id)
                    .header("X-Builder-Session-ID", self.builder_session_id.to_string())
                    .header("X-Signature", signature)
                    .header("X-Timestamp", timestamp)
                    .body(body.clone())
                    .send()
                    .await
                {
                    Ok(response) => response,
                    Err(_) => return Ok(CompletionAttempt::Transient),
                };
                let status = response.status();
                if !matches!(
                    status,
                    reqwest::StatusCode::OK | reqwest::StatusCode::ACCEPTED
                ) {
                    return parse_completion_response(status, &[]);
                }
                let mut response = response;
                let mut bytes = Vec::new();
                loop {
                    let chunk = match response.chunk().await {
                        Ok(Some(chunk)) => chunk,
                        Ok(None) => break,
                        Err(_) => return Ok(CompletionAttempt::Transient),
                    };
                    // Completion metadata is small. Bound hostile response buffering.
                    if bytes.len().saturating_add(chunk.len()) > 16 * 1024 {
                        anyhow::bail!("Completion acknowledgement exceeds size limit");
                    }
                    bytes.extend_from_slice(&chunk);
                }
                parse_completion_response(status, &bytes)
            })
            .await;
            let delay = match result {
                Ok(Ok(CompletionAttempt::Accepted(receipt))) => return Ok(receipt),
                Ok(Ok(CompletionAttempt::Pending(code, delay))) => {
                    pending_code = Some(code);
                    match delay {
                        Some(delay) => delay.max(COMPLETION_REPORT_MIN_BACKOFF),
                        None => break,
                    }
                }
                Ok(Ok(CompletionAttempt::Transient)) | Err(_) => {
                    last_error =
                        anyhow::anyhow!("Completion report transport or server unavailable");
                    COMPLETION_REPORT_MIN_BACKOFF.saturating_mul(1 << (attempts - 1))
                }
                Ok(Err(error)) => {
                    last_error = error;
                    break;
                }
            };
            if attempts == COMPLETION_REPORT_MAX_ATTEMPTS {
                break;
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if delay >= remaining {
                break;
            }
            tokio::time::sleep(delay).await;
        }
        // INVARIANT: A durable success acknowledgement cannot be downgraded by
        // a later transport error, rejection, or reporting deadline.
        match pending_code {
            Some(code) => Ok(BuildCompletionReceipt::PublicationPending { code, attempts }),
            None => Err(last_error),
        }
    }

    /// Fail a job with an error message
    pub async fn fail_job(&self, job_id: uuid::Uuid, error_message: &str) -> Result<()> {
        self.fail_job_with_phase(
            job_id,
            BuildFailurePhase::Build,
            BuildFailureClass::Unknown,
            error_message,
        )
        .await
    }

    /// Fail a job with an explicit remote-build phase classification.
    pub async fn fail_job_with_phase(
        &self,
        job_id: uuid::Uuid,
        phase: BuildFailurePhase,
        failure_class: BuildFailureClass,
        error_message: &str,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct FailRequest {
            status: &'static str,
            failure_phase: String,
            failure_class: BuildFailureClass,
            error_message: String,
        }

        let path = format!("/api/v1/builders/{}/jobs/{}/fail", self.builder_id, job_id);
        let url = format!("{}{}", self.server_url, path);
        let request = FailRequest {
            status: "failed",
            failure_phase: phase.to_string(),
            failure_class,
            error_message: error_message.to_string(),
        };
        let body = serde_json::to_vec(&request)?;
        let (builder_id, signature, timestamp) = self.sign_request("POST", &path, &body);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .body(body)
            .send()
            .await
            .context("Failed to fail job")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!(
                "Fail job request failed with status {}: {}",
                status,
                error_text
            );
        }

        info!("Job {} marked as failed during {}", job_id, phase);
        Ok(())
    }

    /// Poll the current status of a job (HTTP fallback for cancel detection).
    pub async fn get_job_status(&self, job_id: uuid::Uuid) -> Result<Option<String>> {
        let path = format!(
            "/api/v1/builders/{}/jobs/{}/status",
            self.builder_id, job_id
        );
        let url = format!("{}{}", self.server_url, path);
        let (builder_id, signature, timestamp) = self.sign_request("GET", &path, &[]);

        let response = self
            .client
            .get(&url)
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .send()
            .await
            .context("Failed to poll job status")?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!(
                "Get job status failed with status {}: {}",
                status,
                error_text
            );
        }

        let value: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse job status response")?;

        Ok(value
            .get("status")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string()))
    }

    /// Notify the server that a cancelling job has been fully stopped.
    pub async fn finalize_cancelled_job(&self, job_id: uuid::Uuid) -> Result<()> {
        let path = format!(
            "/api/v1/builders/{}/jobs/{}/finalize-cancelled",
            self.builder_id, job_id
        );
        let url = format!("{}{}", self.server_url, path);
        let (builder_id, signature, timestamp) = self.sign_request("POST", &path, &[]);

        let response = self
            .client
            .post(&url)
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .body("")
            .send()
            .await
            .context("Failed to finalize cancelled job")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!(
                "Finalize-cancelled failed with status {}: {}",
                status,
                error_text
            );
        }

        info!("Job {} finalized as cancelled", job_id);
        Ok(())
    }

    /// Append logs to a job
    pub async fn append_logs(
        &self,
        job_id: uuid::Uuid,
        log_lines: &str,
    ) -> Result<AppendLogsOutcome> {
        #[derive(Serialize)]
        struct LogRequest {
            logs: String,
        }

        let path = format!("/api/v1/builders/{}/jobs/{}/logs", self.builder_id, job_id);
        let url = format!("{}{}", self.server_url, path);
        let request = LogRequest {
            logs: log_lines.to_string(),
        };
        let body = serde_json::to_vec(&request)?;
        let (builder_id, signature, timestamp) = self.sign_request("POST", &path, &body);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Builder-ID", builder_id)
            .header("X-Builder-Session-ID", self.builder_session_id.to_string())
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .body(body)
            .send()
            .await
            .context("Failed to append logs")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            warn!("Append logs failed with status {}: {}", status, error_text);
            return Ok(append_logs_outcome_for_status(status, &error_text));
        }

        debug!("Logs appended to job {}", job_id);
        Ok(AppendLogsOutcome::Appended)
    }

    /// Create WebSocket URL for log streaming
    fn ws_url(&self, job_id: &Uuid) -> String {
        let base = self
            .server_url
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        format!("{}/api/v1/build-jobs/{}/logs/stream", base, job_id)
    }

    /// Returns a WebSocket stream for streaming build logs and metrics
    pub async fn create_log_stream(
        &self,
        job_id: &Uuid,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    > {
        let ws_url = self.ws_url(job_id);
        info!("🔌 Connecting WebSocket to {}", ws_url);

        let path = format!("/api/v1/build-jobs/{}/logs/stream", job_id);
        let (builder_id, signature, timestamp) = self.sign_request("GET", &path, &[]);

        let mut request = ws_url
            .clone()
            .into_client_request()
            .context("Failed to build WebSocket request")?;
        request.headers_mut().insert(
            "X-Builder-ID",
            builder_id.parse().context("invalid X-Builder-ID header")?,
        );
        request.headers_mut().insert(
            "X-Builder-Session-ID",
            self.builder_session_id
                .to_string()
                .parse()
                .context("invalid X-Builder-Session-ID header")?,
        );
        request.headers_mut().insert(
            "X-Signature",
            signature.parse().context("invalid X-Signature header")?,
        );
        request.headers_mut().insert(
            "X-Timestamp",
            timestamp.parse().context("invalid X-Timestamp header")?,
        );

        let (ws_stream, _) = connect_async(request)
            .await
            .context("Failed to connect WebSocket")?;

        info!("✅ WebSocket connected for job {}", job_id);
        Ok(ws_stream)
    }

    /// Send a log line via WebSocket stream
    pub async fn send_log_line(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        line: &str,
    ) -> Result<()> {
        let payload = BuildStreamMessage::Log {
            message: line.to_string(),
        };
        ws.send(Message::Text(serde_json::to_string(&payload)?))
            .await
            .context("Failed to send log line")?;
        Ok(())
    }

    /// Send system metrics via WebSocket stream
    pub async fn send_metrics(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        cpu_percent: f32,
        ram_used_mb: u64,
        ram_total_mb: u64,
    ) -> Result<()> {
        let metrics = BuildStreamMessage::Metrics {
            cpu_percent,
            ram_used_mb,
            ram_total_mb,
            timestamp: Utc::now().to_rfc3339(),
        };

        let json = serde_json::to_string(&metrics)?;
        ws.send(Message::Text(json))
            .await
            .context("Failed to send metrics")?;
        Ok(())
    }
}

/// API-backed build reporter for remote builders.
///
/// Reports progress and checks cancellation entirely over the server API with no
/// database access. Progress is sent via HTTP POST; cancellation is detected by
/// polling the job status endpoint. This keeps remote builders fully DB-free.
#[derive(Clone)]
pub struct ApiBuildReporter {
    client: BuilderApiClient,
    job_id: Uuid,
}

impl ApiBuildReporter {
    pub fn new(client: BuilderApiClient, job_id: Uuid) -> Self {
        Self { client, job_id }
    }
}

#[async_trait::async_trait]
impl crate::build::BuildReporter for ApiBuildReporter {
    async fn report_progress(&self, progress: &crate::build::BuildProgress) -> Result<()> {
        let request = BuildProgressRequest {
            derivation_id: progress.derivation_id,
            elapsed_seconds: progress.elapsed_seconds,
            current_target: progress.current_target.clone(),
            last_activity_seconds: progress.last_activity_seconds,
        };
        self.client.report_progress(self.job_id, &request).await
    }

    async fn is_cancelled(&self, job_id: Option<Uuid>) -> Result<bool> {
        let Some(job_id) = job_id else {
            return Ok(false);
        };
        let status = self.client.get_job_status(job_id).await?;
        Ok(job_status_requests_cancellation(status.as_deref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn niks3_next_job_poll_signs_runtime_capability_without_cve_support() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let key = SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = key.verifying_key();
        let client = BuilderApiClient {
            client: Client::builder().no_proxy().build().unwrap(),
            server_url: format!("http://{address}"),
            builder_id: Uuid::new_v4(),
            builder_session_id: Uuid::new_v4(),
            signing_key: key,
            supported_execution_strategies: vec![RemoteBuildExecutionStrategy::ServerDerivation],
            supported_evaluator_contract_versions: Vec::new(),
            evaluator: None,
            capabilities: BuilderCapabilities {
                niks3_cache: true,
                ..Default::default()
            },
        };
        let path = format!("/api/v1/builders/{}/next-job", client.builder_id);
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let header_end = loop {
                let mut chunk = [0; 1024];
                let size = stream.read(&mut chunk).await.unwrap();
                assert_ne!(size, 0);
                bytes.extend_from_slice(&chunk[..size]);
                if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
            assert!(headers.starts_with(&format!("POST {path} HTTP/1.1")));
            let header = |name: &str| {
                headers
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case(name)
                            .then(|| value.trim().to_owned())
                    })
                    .unwrap()
            };
            let length = header("content-length").parse::<usize>().unwrap();
            while bytes.len() < header_end + length {
                let mut chunk = [0; 1024];
                let size = stream.read(&mut chunk).await.unwrap();
                assert_ne!(size, 0);
                bytes.extend_from_slice(&chunk[..size]);
            }
            let body = &bytes[header_end..header_end + length];
            let request: NextJobRequest = serde_json::from_slice(body).unwrap();
            assert!(request.capabilities.niks3_cache);
            assert!(!request.capabilities.supports_current_cve_schema());
            let payload = BuilderApiClient::canonical_signature_payload(
                "POST",
                &path,
                &header("x-timestamp"),
                body,
            );
            let signature = ed25519_dalek::Signature::from_slice(
                &base64::engine::general_purpose::STANDARD
                    .decode(header("x-signature"))
                    .unwrap(),
            )
            .unwrap();
            verifying_key.verify_strict(&payload, &signature).unwrap();
            stream
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });
        assert!(client.get_next_job().await.unwrap().is_none());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn niks3_completion_signs_destination_id_and_omits_it_without_push() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();
        let job_id = Uuid::new_v4();
        let client = BuilderApiClient {
            capabilities: Default::default(),
            client: Client::builder().no_proxy().build().unwrap(),
            server_url: format!("http://{address}"),
            builder_id: Uuid::new_v4(),
            builder_session_id: Uuid::new_v4(),
            signing_key,
            supported_execution_strategies: Vec::new(),
            supported_evaluator_contract_versions: Vec::new(),
            evaluator: None,
        };
        let path = format!(
            "/api/v1/builders/{}/jobs/{job_id}/complete",
            client.builder_id
        );
        let server = tokio::spawn(async move {
            for expected_id in [Some(470), None] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut chunk = [0; 1024];
                    let size = stream.read(&mut chunk).await.unwrap();
                    assert_ne!(size, 0);
                    bytes.extend_from_slice(&chunk[..size]);
                    if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
                assert!(headers.starts_with(&format!("POST {path} HTTP/1.1")));
                let header = |name: &str| {
                    headers
                        .lines()
                        .find_map(|line| {
                            let (key, value) = line.split_once(':')?;
                            key.eq_ignore_ascii_case(name).then_some(value.trim())
                        })
                        .unwrap()
                };
                let length: usize = header("content-length").parse().unwrap();
                while bytes.len() < header_end + length {
                    let mut chunk = [0; 1024];
                    let size = stream.read(&mut chunk).await.unwrap();
                    assert_ne!(size, 0);
                    bytes.extend_from_slice(&chunk[..size]);
                }
                let body = &bytes[header_end..header_end + length];
                let completion: cf_protocol::builder::CompleteJobRequest =
                    serde_json::from_slice(body).unwrap();
                assert_eq!(completion.cache_destination_id, expected_id);
                assert_eq!(completion.cache_pushed, expected_id.is_some());
                assert_eq!(completion.cache_reference.is_some(), expected_id.is_some());
                let signature = base64::engine::general_purpose::STANDARD
                    .decode(header("x-signature"))
                    .unwrap();
                verifying_key
                    .verify_strict(
                        &BuilderApiClient::canonical_signature_payload(
                            "POST",
                            &path,
                            header("x-timestamp"),
                            body,
                        ),
                        &Signature::from_slice(&signature).unwrap(),
                    )
                    .unwrap();
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await
                    .unwrap();
            }
        });
        client
            .complete_job(
                job_id,
                "/nix/store/output",
                Some("https://read.example.org"),
                Some(470),
            )
            .await
            .unwrap();
        client
            .complete_job(job_id, "/nix/store/output", None, Some(470))
            .await
            .unwrap();
        server.await.unwrap();
    }

    fn pending_response(code: &str, retryable: bool, delay: u64) -> String {
        serde_json::json!({
            "build_status": "success", "publication_status": "pending",
            "verification_code": code, "retryable": retryable,
            "retry_after_secs": delay,
            "error": "untrusted-secret-https://signed.example/?token=private"
        })
        .to_string()
    }

    // A real TCP mock verifies every request's canonical signature and exact
    // retained identity. It accepts completion reports only, never work APIs.
    async fn completion_sequence(responses: Vec<(u16, String)>) -> Result<BuildCompletionReceipt> {
        completion_sequence_with_timeouts(responses, None).await
    }

    async fn completion_sequence_with_timeouts(
        responses: Vec<(u16, String)>,
        limits: Option<(std::time::Duration, std::time::Duration)>,
    ) -> Result<BuildCompletionReceipt> {
        completion_sequence_for_reference(responses, limits, "https://read.example.org", Some(470))
            .await
    }

    async fn completion_sequence_for_reference(
        responses: Vec<(u16, String)>,
        limits: Option<(std::time::Duration, std::time::Duration)>,
        reference: &'static str,
        destination_id: Option<i32>,
    ) -> Result<BuildCompletionReceipt> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();
        let client = BuilderApiClient {
            client: Client::builder().no_proxy().build().unwrap(),
            server_url: format!("http://{}", listener.local_addr().unwrap()),
            builder_id: Uuid::new_v4(),
            builder_session_id: Uuid::new_v4(),
            signing_key,
            supported_execution_strategies: Vec::new(),
            supported_evaluator_contract_versions: Vec::new(),
            evaluator: None,
            capabilities: Default::default(),
        };
        let job_id = Uuid::new_v4();
        let path = format!(
            "/api/v1/builders/{}/jobs/{job_id}/complete",
            client.builder_id
        );
        let request_path = path.clone();
        let session = client.builder_session_id.to_string();
        let builder_id = client.builder_id.to_string();
        let server = tokio::spawn(async move {
            let mut previous_body = None;
            let mut previous_timestamp = None;
            let mut previous_signature = None;
            for (status, response) in responses {
                let (mut stream, _) =
                    tokio::time::timeout(std::time::Duration::from_secs(40), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut chunk = [0; 1024];
                    let length = stream.read(&mut chunk).await.unwrap();
                    assert_ne!(length, 0);
                    bytes.extend_from_slice(&chunk[..length]);
                    if let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
                assert!(headers.starts_with(&format!("POST {path} HTTP/1.1")));
                let header = |name: &str| {
                    headers
                        .lines()
                        .find_map(|line| {
                            let (key, value) = line.split_once(':')?;
                            key.eq_ignore_ascii_case(name)
                                .then(|| value.trim().to_owned())
                        })
                        .unwrap()
                };
                let length: usize = header("content-length").parse().unwrap();
                while bytes.len() < header_end + length {
                    let mut chunk = [0; 1024];
                    let length = stream.read(&mut chunk).await.unwrap();
                    assert_ne!(length, 0);
                    bytes.extend_from_slice(&chunk[..length]);
                }
                let body = bytes[header_end..header_end + length].to_vec();
                let request: cf_protocol::builder::CompleteJobRequest =
                    serde_json::from_slice(&body).unwrap();
                assert_eq!(
                    request.output_path.as_deref(),
                    Some("/nix/store/authoritative-output")
                );
                assert_eq!(request.cache_destination_id, destination_id);
                // Do not format even synthetic credential-bearing payloads on
                // assertion failure. Only the equality result is diagnostic.
                assert!(request.cache_reference.as_deref() == Some(reference));
                assert!(request.cache_pushed);
                assert_eq!(header("x-builder-session-id"), session);
                assert_eq!(header("x-builder-id"), builder_id);
                let timestamp = header("x-timestamp");
                let signature = header("x-signature");
                verifying_key
                    .verify_strict(
                        &BuilderApiClient::canonical_signature_payload(
                            "POST", &path, &timestamp, &body,
                        ),
                        &Signature::from_slice(
                            &base64::engine::general_purpose::STANDARD
                                .decode(&signature)
                                .unwrap(),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                if let Some(previous) = previous_body {
                    assert!(body == previous);
                    assert_ne!(Some(&timestamp), previous_timestamp.as_ref());
                    assert_ne!(Some(&signature), previous_signature.as_ref());
                    let earlier =
                        chrono::DateTime::parse_from_rfc3339(previous_timestamp.as_ref().unwrap())
                            .unwrap();
                    let current = chrono::DateTime::parse_from_rfc3339(&timestamp).unwrap();
                    assert!((current - earlier).num_milliseconds() >= 2000);
                }
                previous_body = Some(body);
                previous_timestamp = Some(timestamp);
                previous_signature = Some(signature);
                // Status one stalls the response body until the caller drops
                // its request; zero closes without any acknowledgement.
                if status == 1 {
                    stream
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\n")
                        .await
                        .unwrap();
                    assert_eq!(stream.read(&mut [0; 1]).await.unwrap(), 0);
                } else if status != 0 {
                    stream.write_all(format!(
                        "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()
                    ).as_bytes()).await.unwrap();
                }
            }
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(50), listener.accept())
                    .await
                    .is_err()
            );
        });
        let result = match limits {
            None => {
                client
                    .complete_job(
                        job_id,
                        "/nix/store/authoritative-output",
                        Some(reference),
                        destination_id,
                    )
                    .await
            }
            Some((budget, attempt_timeout)) => {
                let body = serde_json::to_vec(&cf_protocol::builder::CompleteJobRequest {
                    output_path: Some("/nix/store/authoritative-output".into()),
                    cache_pushed: true,
                    cache_reference: Some(reference.into()),
                    cache_destination_id: destination_id,
                })
                .unwrap();
                client
                    .report_completion_request(&request_path, body, budget, attempt_timeout)
                    .await
            }
        };
        server.await.unwrap();
        result
    }

    #[tokio::test]
    async fn completion_pending_retries_fresh_signatures_then_confirms() {
        let receipt = completion_sequence(vec![
            (202, pending_response("cache_path_missing", true, 0)),
            (
                200,
                r#"{"build_status":"success","publication_status":"confirmed"}"#.into(),
            ),
        ])
        .await
        .unwrap();
        assert_eq!(receipt, BuildCompletionReceipt::PublicationConfirmed);
        assert!(receipt.permits_post_build_scan(true));
    }

    #[tokio::test]
    async fn completion_pending_exhaustion_is_durable_success_after_five_requests() {
        let receipt = completion_sequence(
            (0..5)
                .map(|_| (202, pending_response("cache_network_error", true, 0)))
                .collect(),
        )
        .await
        .unwrap();
        assert_eq!(
            receipt,
            BuildCompletionReceipt::PublicationPending {
                code: PublicationVerificationCode::CacheNetworkError,
                attempts: 5,
            }
        );
        assert!(!receipt.permits_post_build_scan(true));
        assert!(receipt.permits_post_build_scan(false));
    }

    #[tokio::test]
    async fn completion_permanent_codes_and_retry_false_do_not_retry() {
        for (code, retryable) in [
            ("cache_authentication_failed", true),
            ("cache_read_configuration_invalid", true),
            ("cache_probe_failed", true),
            ("cache_rate_limited", false),
        ] {
            let receipt = completion_sequence(vec![(202, pending_response(code, retryable, 0))])
                .await
                .unwrap();
            assert!(matches!(
                receipt,
                BuildCompletionReceipt::PublicationPending { attempts: 1, .. }
            ));
        }
    }

    #[tokio::test]
    async fn completion_retry_after_beyond_budget_returns_pending_without_waiting() {
        assert!(matches!(
            completion_sequence(vec![(
                202,
                pending_response("cache_rate_limited", true, u64::MAX)
            )])
            .await
            .unwrap(),
            BuildCompletionReceipt::PublicationPending { attempts: 1, .. }
        ));
    }

    #[tokio::test]
    async fn completion_transport_retry_preserves_acknowledged_pending_on_rejection() {
        let receipt = completion_sequence(vec![
            (0, String::new()),
            (202, pending_response("cache_probe_timeout", true, 0)),
            (403, "untrusted-secret".into()),
        ])
        .await
        .unwrap();
        assert_eq!(
            receipt,
            BuildCompletionReceipt::PublicationPending {
                code: PublicationVerificationCode::CacheProbeTimeout,
                attempts: 3,
            }
        );
    }

    #[tokio::test]
    async fn completion_errors_never_retain_untrusted_response_text() {
        for (status, body) in [
            (401, "untrusted-secret".into()),
            (409, "untrusted-secret".into()),
            (202, pending_response("untrusted-secret", true, 0)),
            (200, "untrusted-secret".into()),
        ] {
            let error = completion_sequence(vec![(status, body)]).await.unwrap_err();
            assert!(!format!("{error:#}").contains("untrusted-secret"));
        }
    }

    #[tokio::test]
    async fn completion_attempt_and_outer_deadlines_cover_stalled_response_bodies() {
        use std::time::Duration;
        for limits in [
            (Duration::from_secs(1), Duration::from_millis(20)),
            (Duration::from_millis(20), Duration::from_secs(1)),
        ] {
            let start = tokio::time::Instant::now();
            assert!(
                completion_sequence_with_timeouts(vec![(1, String::new())], Some(limits))
                    .await
                    .is_err()
            );
            assert!(start.elapsed() < Duration::from_secs(1));
        }
        let receipt = completion_sequence_with_timeouts(
            vec![
                (202, pending_response("cache_path_missing", true, 0)),
                (1, String::new()),
            ],
            Some((Duration::from_secs(3), Duration::from_millis(100))),
        )
        .await
        .unwrap();
        assert_eq!(
            receipt,
            BuildCompletionReceipt::PublicationPending {
                code: PublicationVerificationCode::CachePathMissing,
                attempts: 2,
            }
        );
        assert_eq!(BUILD_COMPLETION_TIMEOUT, Duration::from_secs(90));
        assert_eq!(COMPLETION_REPORT_BUDGET, Duration::from_secs(600));
    }

    #[test]
    fn completion_response_contract_and_retry_allowlist() {
        assert!(!BuildCompletionReceipt::LegacyAccepted.permits_post_build_scan(true));
        assert!(BuildCompletionReceipt::LegacyAccepted.permits_post_build_scan(false));
        assert!(!BuildCompletionReceipt::PublicationNotRequired.permits_post_build_scan(true));
        for code in [
            "cache_path_missing",
            "cache_rate_limited",
            "cache_network_error",
            "cache_probe_timeout",
        ] {
            assert!(
                matches!(parse_completion_response(reqwest::StatusCode::ACCEPTED,
                pending_response(code, true, 4).as_bytes()).unwrap(), CompletionAttempt::Pending(_, Some(delay)) if delay.as_secs() == 4)
            );
        }
        for body in [
            r#"{}"#,
            r#"{"build_status":"failed","publication_status":"pending"}"#,
            r#"{"build_status":"success","publication_status":"confirmed"}"#,
        ] {
            assert!(
                parse_completion_response(reqwest::StatusCode::ACCEPTED, body.as_bytes()).is_err()
            );
        }
        assert!(matches!(
            parse_completion_response(reqwest::StatusCode::SERVICE_UNAVAILABLE, b"private")
                .unwrap(),
            CompletionAttempt::Transient
        ));
        assert!(matches!(
            parse_completion_response(
                reqwest::StatusCode::OK,
                br#"{"build_status":"success","publication_status":"none"}"#
            )
            .unwrap(),
            CompletionAttempt::Accepted(BuildCompletionReceipt::PublicationNotRequired)
        ));
    }

    #[tokio::test]
    async fn completion_preserves_opaque_authorized_references_without_diagnostic_exposure() {
        for reference in [
            "https://fixture-user:synthetic-sentinel@cache.example",
            "https://cache.example?X-Amz-Signature=synthetic-sentinel",
            "remote:cache-name",
        ] {
            let error = completion_sequence_for_reference(
                vec![(403, reference.into())],
                None,
                reference,
                Some(470),
            )
            .await
            .unwrap_err();
            assert_eq!(error.to_string(), "Completion report rejected (HTTP 403)");
            assert!(!format!("{error:#}").contains("synthetic-sentinel"));
        }
    }

    #[tokio::test]
    async fn completion_preserves_legacy_ssh_username_reference_in_signed_body() {
        let receipt = completion_sequence_for_reference(
            vec![(
                200,
                r#"{"build_status":"success","publication_status":"confirmed"}"#.into(),
            )],
            None,
            "ssh://nix@cache-host",
            None,
        )
        .await
        .unwrap();
        assert_eq!(receipt, BuildCompletionReceipt::PublicationConfirmed);
    }

    #[test]
    fn test_load_valid_private_key() {
        let key = SigningKey::generate(&mut rand::thread_rng());
        let key_bytes = key.to_bytes();

        // Encode key as base64 (matching cf-keygen format)
        let key_base64 = base64::engine::general_purpose::STANDARD.encode(&key_bytes);

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(key_base64.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let loaded_key = BuilderApiClient::load_private_key(temp_file.path()).unwrap();
        assert_eq!(loaded_key.to_bytes(), key_bytes);
    }

    #[test]
    fn test_load_invalid_key_length() {
        // Write base64-encoded invalid key (16 bytes instead of 32)
        let invalid_key = base64::engine::general_purpose::STANDARD.encode(&[0u8; 16]);

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(invalid_key.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let result = BuilderApiClient::load_private_key(temp_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_sign_request() {
        let key = SigningKey::generate(&mut rand::thread_rng());
        let builder_id = Uuid::new_v4();

        let client = BuilderApiClient {
            capabilities: Default::default(),
            client: Client::new(),
            server_url: "http://localhost:8080".to_string(),
            builder_id,
            builder_session_id: Uuid::new_v4(),
            signing_key: key,
            supported_execution_strategies: vec![RemoteBuildExecutionStrategy::ServerDerivation],
            supported_evaluator_contract_versions: Vec::new(),
            evaluator: None,
        };

        let body = b"test request body";
        let (id, sig, ts) = client.sign_request("POST", "/api/v1/test", body);

        assert_eq!(id, builder_id.to_string());
        assert_eq!(sig.len(), 88); // 64 bytes Ed25519 signature as base64
        assert!(!ts.is_empty());
    }

    #[test]
    fn append_logs_409_terminal_is_not_success() {
        assert_eq!(
            append_logs_outcome_for_status(
                reqwest::StatusCode::CONFLICT,
                "Cannot append logs for terminal job in 'cancelled' status",
            ),
            AppendLogsOutcome::TerminalJob
        );
        assert_eq!(
            append_logs_outcome_for_status(reqwest::StatusCode::CONFLICT, "temporary conflict"),
            AppendLogsOutcome::Rejected
        );
        assert_eq!(
            append_logs_outcome_for_status(reqwest::StatusCode::OK, ""),
            AppendLogsOutcome::Appended
        );
    }

    #[test]
    fn cancelling_and_cancelled_statuses_request_cancellation() {
        assert!(job_status_requests_cancellation(Some("cancelling")));
        assert!(job_status_requests_cancellation(Some("cancelled")));
        assert!(!job_status_requests_cancellation(Some("building")));
        assert!(!job_status_requests_cancellation(Some("success")));
        assert!(!job_status_requests_cancellation(None));
    }

    #[tokio::test]
    async fn verified_only_builder_does_not_retry_legacy_get_after_405() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let addr = listener
            .local_addr()
            .expect("listener address should exist");
        let request_count = Arc::new(AtomicUsize::new(0));
        let request_count_for_server = Arc::clone(&request_count);

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("client should connect once");
            request_count_for_server.fetch_add(1, Ordering::SeqCst);
            let mut buf = [0_u8; 1024];
            let n = stream
                .read(&mut buf)
                .await
                .expect("request should be readable");
            let request = String::from_utf8_lossy(&buf[..n]);
            assert!(
                request.starts_with("POST /api/v1/builders/"),
                "initial next-job request should be POST, got: {request}"
            );
            stream
                .write_all(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n")
                .await
                .expect("response should write");
        });

        let client = BuilderApiClient {
            client: Client::new(),
            server_url: format!("http://{addr}"),
            capabilities: Default::default(),
            builder_id: Uuid::new_v4(),
            builder_session_id: Uuid::new_v4(),
            signing_key: SigningKey::generate(&mut rand::thread_rng()),
            supported_execution_strategies: vec![
                RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
            ],
            supported_evaluator_contract_versions: vec![
                cf_protocol::builder::VERIFIED_SOURCE_EVALUATOR_CONTRACT_VERSION,
            ],
            evaluator: Some(EvaluatorFingerprint {
                contract_version: cf_protocol::builder::VERIFIED_SOURCE_EVALUATOR_CONTRACT_VERSION,
                nix_version: "test".to_string(),
                evaluator_system: "x86_64-linux".to_string(),
                pure_eval: true,
                lockfile_mutation_allowed: false,
                allow_import_from_derivation: true,
                source_materialization_schema_version:
                    cf_protocol::builder::VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION,
            }),
        };

        let result = client.get_next_job().await;

        assert!(
            result.is_err(),
            "verified-only builder should reject legacy fallback"
        );
        assert!(
            result
                .expect_err("result should be an error")
                .to_string()
                .contains("does not support server_derivation")
        );
        server.await.expect("test server should finish");
        assert_eq!(
            request_count.load(Ordering::SeqCst),
            1,
            "verified-only builder must not issue a legacy GET retry"
        );
    }

    #[test]
    fn test_bootstrap_signature_uses_public_key_identity_without_builder_id() {
        let key = SigningKey::generate(&mut rand::thread_rng());
        let public_key = BuilderApiClient::public_key_base64_for(&key);
        let body = serde_json::to_vec(&ResolveBuilderIdRequest {
            public_key,
            session_id: Some(Uuid::new_v4()),
            capabilities: BuilderCapabilities::default(),
        })
        .unwrap();

        let (signature, timestamp) = BuilderApiClient::sign_bootstrap_request(
            &key,
            "POST",
            "/api/v1/builders/resolve-id",
            &body,
        );

        assert_eq!(signature.len(), 88); // 64 bytes Ed25519 signature as base64
        assert!(!timestamp.is_empty());
    }

    #[test]
    fn build_stream_log_frame_uses_explicit_type() {
        let msg = BuildStreamMessage::Log {
            message: "line".to_string(),
        };

        let encoded = serde_json::to_string(&msg).expect("message should encode");
        assert!(encoded.contains("\"type\":\"log\""));
        assert!(encoded.contains("\"message\":\"line\""));
    }

    #[test]
    fn build_stream_metrics_frame_uses_explicit_type() {
        let msg = BuildStreamMessage::Metrics {
            cpu_percent: 12.5,
            ram_used_mb: 512,
            ram_total_mb: 2048,
            timestamp: "2026-03-02T17:00:00Z".to_string(),
        };

        let encoded = serde_json::to_string(&msg).expect("message should encode");
        assert!(encoded.contains("\"type\":\"metrics\""));
        assert!(encoded.contains("\"cpu_percent\":12.5"));
    }
}
