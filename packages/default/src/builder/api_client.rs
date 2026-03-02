use crate::config::BuilderConfig;
use crate::models::builders::{BuildJob, ReportMetricsRequest};
use anyhow::{Context, Result};
use base64::Engine;
use chrono::Utc;
use ed25519_dalek::{Signature, Signer, SigningKey};
use futures::SinkExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tokio_tungstenite::{connect_async, tungstenite::{Message, client::IntoClientRequest}};
use tracing::{debug, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BuildStreamMessage {
    Log { message: String },
    Metrics {
        cpu_percent: f32,
        ram_used_mb: u64,
        ram_total_mb: u64,
        timestamp: String,
    },
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
    /// Create a new API client from configuration
    pub async fn new(config: &BuilderConfig) -> Result<Self> {
        let builder_id = config.require_builder_id()?;
        let key_path = config.require_private_key_path()?;
        let server_url = config.require_server_url()?;

        // Load private key from file
        let signing_key =
            Self::load_private_key(&key_path).context("Failed to load builder private key")?;

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            server_url,
            builder_id,
            signing_key,
        })
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

    /// Get the next available job from the server
    pub async fn get_next_job(&self) -> Result<Option<BuildJob>> {
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

        let job: BuildJob = response
            .json()
            .await
            .context("Failed to parse job response")?;

        Ok(Some(job))
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
        #[derive(Serialize)]
        struct FailRequest {
            error_message: String,
        }

        let path = format!("/api/v1/builders/{}/jobs/{}/fail", self.builder_id, job_id);
        let url = format!("{}{}", self.server_url, path);
        let request = FailRequest {
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

        info!("Job {} marked as failed", job_id);
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
        let base = self.server_url.replace("http://", "ws://").replace("https://", "wss://");
        format!("{}/api/v1/build-jobs/{}/logs/stream", base, job_id)
    }

    /// Stream a log line via WebSocket
    /// Returns a WebSocket stream that can be used to send log lines and metrics
    pub async fn create_log_stream(&self, job_id: &Uuid) -> Result<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>> {
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
        ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
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
        ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
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
