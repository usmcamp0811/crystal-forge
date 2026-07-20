use cf_config::config::BuilderConfig;
use cf_protocol::builder::{
    BuildFailurePhase, BuildProgressRequest, EstablishBuilderSessionRequest,
    EstablishBuilderSessionResponse, NextJobRequest, NextJobResponse, RemoteBuildExecutionStrategy,
    ReportMetricsRequest, ResolveBuilderIdRequest, ResolveBuilderIdResponse,
};
use anyhow::{Context, Result};
use base64::Engine;
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

const DEFAULT_API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const DERIVATION_ARCHIVE_DOWNLOAD_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(30 * 60);

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
    pub async fn new(config: &BuilderConfig) -> Result<Self> {
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
        })
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
    ) -> Result<Uuid> {
        let public_key = Self::public_key_base64_for(signing_key);
        let mut delay = retry_interval.max(std::time::Duration::from_secs(1));
        let mut attempt: u32 = 0;

        loop {
            attempt += 1;
            match Self::resolve_builder_id(client, server_url, signing_key, builder_session_id)
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
    ) -> Result<()> {
        let path = format!("/api/v1/builders/{}/session", builder_id);
        let url = format!("{}{}", server_url, path);
        let body = serde_json::to_vec(&EstablishBuilderSessionRequest {
            session_id: builder_session_id,
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
    ) -> Result<Uuid> {
        let path = "/api/v1/builders/resolve-id";
        let url = format!("{}{}", server_url, path);
        let body = serde_json::to_vec(&ResolveBuilderIdRequest {
            public_key: Self::public_key_base64_for(signing_key),
            session_id: Some(builder_session_id),
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
    pub async fn get_next_job(&self) -> Result<Option<NextJobResponse>> {
        let body = serde_json::to_vec(&NextJobRequest {
            protocol_version: 2,
            supported_execution_strategies: self.supported_execution_strategies.clone(),
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
            warn!(
                "⚠️  Server reports incompatible execution strategy (409 Conflict). \
                 Check server's remote_build_execution_strategy setting matches \
                 builder supported_strategies={:?}",
                self.supported_execution_strategies,
            );
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

    /// Stream the source archive (tar.gz of the bare mirror) for a job using
    /// ServerBundledArchive delivery directly to a temp file, verifying the
    /// SHA-256 incrementally without buffering the whole archive in RAM.
    ///
    /// Returns the path of the downloaded temp file. Callers are responsible
    /// for extracting it and removing it afterwards.
    pub async fn stream_source_archive_to_tempfile(
        &self,
        job_id: uuid::Uuid,
        expected_sha256: Option<&str>,
        dest_dir: &std::path::Path,
    ) -> Result<std::path::PathBuf> {
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
            .context("Failed to request source archive")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!(
                "Download source archive failed with status {}: {}",
                status,
                error_text
            );
        }

        // Stream to a temp file, computing SHA-256 on the fly.
        tokio::fs::create_dir_all(dest_dir)
            .await
            .context("Failed to create source archive temp directory")?;
        let tmp_path = dest_dir.join(format!("source-archive-{job_id}.tar.gz.tmp"));
        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .context("Failed to create source archive temp file")?;

        let mut hasher = Sha256::new();
        let mut byte_stream = response.bytes_stream();
        use futures::StreamExt;
        while let Some(chunk) = byte_stream.next().await {
            let chunk = chunk.context("Error reading source archive chunk from server")?;
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .context("Failed to write source archive chunk to temp file")?;
        }
        file.flush()
            .await
            .context("Failed to flush source archive temp file")?;
        drop(file);

        // Verify SHA-256 if the server provided one.
        if let Some(expected) = expected_sha256 {
            let actual = format!("{:x}", hasher.finalize());
            if actual != expected {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                anyhow::bail!("source archive SHA-256 mismatch: expected {expected}, got {actual}");
            }
            info!("✅ Source archive SHA-256 verified: {}", actual);
        }

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

    /// Complete a job successfully
    pub async fn complete_job(
        &self,
        job_id: uuid::Uuid,
        output_path: &str,
        cache_reference: Option<&str>,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct CompleteRequest {
            output_path: String,
            cache_pushed: bool,
            cache_reference: Option<String>,
        }

        let path = format!(
            "/api/v1/builders/{}/jobs/{}/complete",
            self.builder_id, job_id
        );
        let url = format!("{}{}", self.server_url, path);
        let request = CompleteRequest {
            output_path: output_path.to_string(),
            cache_pushed: cache_reference.is_some(),
            cache_reference: cache_reference.map(ToString::to_string),
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
            .context("Failed to complete job")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!("Complete job failed with status {}: {}", status, error_text);
        }

        info!("Job {} completed successfully", job_id);
        Ok(())
    }

    /// Fail a job with an error message
    pub async fn fail_job(&self, job_id: uuid::Uuid, error_message: &str) -> Result<()> {
        self.fail_job_with_phase(job_id, BuildFailurePhase::Build, error_message)
            .await
    }

    /// Fail a job with an explicit remote-build phase classification.
    pub async fn fail_job_with_phase(
        &self,
        job_id: uuid::Uuid,
        phase: BuildFailurePhase,
        error_message: &str,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct FailRequest {
            status: &'static str,
            failure_phase: String,
            error_message: String,
        }

        let path = format!("/api/v1/builders/{}/jobs/{}/fail", self.builder_id, job_id);
        let url = format!("{}{}", self.server_url, path);
        let request = FailRequest {
            status: "failed",
            failure_phase: phase.to_string(),
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
    pub async fn append_logs(&self, job_id: uuid::Uuid, log_lines: &str) -> Result<()> {
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

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            warn!("Append logs failed with status {}: {}", status, error_text);
            // Don't fail the entire operation if log append fails
        }

        debug!("Logs appended to job {}", job_id);
        Ok(())
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

/// API-backed [`BuildReporter`] for remote builders.
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
    async fn report_progress(
        &self,
        progress: &crate::build::BuildProgress,
    ) -> Result<()> {
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
        Ok(matches!(status.as_deref(), Some("cancelling")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

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
            client: Client::new(),
            server_url: "http://localhost:8080".to_string(),
            builder_id,
            builder_session_id: Uuid::new_v4(),
            signing_key: key,
            supported_execution_strategies: vec![RemoteBuildExecutionStrategy::ServerDerivation],
        };

        let body = b"test request body";
        let (id, sig, ts) = client.sign_request("POST", "/api/v1/test", body);

        assert_eq!(id, builder_id.to_string());
        assert_eq!(sig.len(), 88); // 64 bytes Ed25519 signature as base64
        assert!(!ts.is_empty());
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
            builder_id: Uuid::new_v4(),
            builder_session_id: Uuid::new_v4(),
            signing_key: SigningKey::generate(&mut rand::thread_rng()),
            supported_execution_strategies: vec![
                RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
            ],
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
