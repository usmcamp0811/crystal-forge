use crate::config::BuilderConfig;
use crate::models::builders::{
    BuildFailurePhase, BuildProgressRequest, NextJobResponse, ReportMetricsRequest,
    ResolveBuilderIdRequest, ResolveBuilderIdResponse,
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
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tracing::{debug, info, warn};
use uuid::Uuid;

const DEFAULT_API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const DERIVATION_ARCHIVE_DOWNLOAD_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(30 * 60);

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
    signing_key: SigningKey,
}

impl BuilderApiClient {
    pub fn builder_id(&self) -> Uuid {
        self.builder_id
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

        let builder_id = match config.builder_id {
            Some(builder_id) => builder_id,
            None => {
                Self::resolve_builder_id_with_retry(
                    &client,
                    &server_url,
                    &signing_key,
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
            signing_key,
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
        retry_interval: std::time::Duration,
        max_interval: std::time::Duration,
        max_attempts: u32,
    ) -> Result<Uuid> {
        let public_key = Self::public_key_base64_for(signing_key);
        let mut delay = retry_interval.max(std::time::Duration::from_secs(1));
        let mut attempt: u32 = 0;

        loop {
            attempt += 1;
            match Self::resolve_builder_id(client, server_url, signing_key).await {
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
    ) -> Result<Uuid> {
        let path = "/api/v1/builders/resolve-id";
        let url = format!("{}{}", server_url, path);
        let body = serde_json::to_vec(&ResolveBuilderIdRequest {
            public_key: Self::public_key_base64_for(signing_key),
        })?;
        let (signature, timestamp) = Self::sign_bootstrap_request(signing_key, "POST", path, &body);

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
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
        let path = format!("/api/v1/builders/{}/next-job", self.builder_id);
        let url = format!("{}{}", self.server_url, path);
        let body = Vec::new(); // Empty body for GET
        let (builder_id, signature, timestamp) = self.sign_request("GET", &path, &body);

        let response = self
            .client
            .get(&url)
            .header("X-Builder-ID", builder_id)
            .header("X-Signature", signature)
            .header("X-Timestamp", timestamp)
            .send()
            .await
            .context("Failed to request next job")?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            // No jobs available
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
    pub async fn complete_job(&self, job_id: uuid::Uuid, output_path: &str) -> Result<()> {
        #[derive(Serialize)]
        struct CompleteRequest {
            output_path: String,
        }

        let path = format!(
            "/api/v1/builders/{}/jobs/{}/complete",
            self.builder_id, job_id
        );
        let url = format!("{}{}", self.server_url, path);
        let request = CompleteRequest {
            output_path: output_path.to_string(),
        };
        let body = serde_json::to_vec(&request)?;
        let (builder_id, signature, timestamp) = self.sign_request("POST", &path, &body);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Builder-ID", builder_id)
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

#[axum::async_trait]
impl crate::derivations::reporter::BuildReporter for ApiBuildReporter {
    async fn report_progress(
        &self,
        progress: &crate::derivations::reporter::BuildProgress,
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
            signing_key: key,
        };

        let body = b"test request body";
        let (id, sig, ts) = client.sign_request("POST", "/api/v1/test", body);

        assert_eq!(id, builder_id.to_string());
        assert_eq!(sig.len(), 88); // 64 bytes Ed25519 signature as base64
        assert!(!ts.is_empty());
    }

    #[test]
    fn test_bootstrap_signature_uses_public_key_identity_without_builder_id() {
        let key = SigningKey::generate(&mut rand::thread_rng());
        let public_key = BuilderApiClient::public_key_base64_for(&key);
        let body = serde_json::to_vec(&ResolveBuilderIdRequest { public_key }).unwrap();

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
