use crate::config::BuilderConfig;
use crate::models::builders::{BuildJob, ReportMetricsRequest};
use anyhow::{Context, Result};
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

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
        let signing_key = Self::load_private_key(&key_path)
            .context("Failed to load builder private key")?;
        
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
        let key_data = fs::read(path)
            .context("Failed to read private key file")?;
        
        if key_data.len() != 32 {
            anyhow::bail!("Invalid private key file: expected 32 bytes, got {}", key_data.len());
        }
        
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&key_data);
        
        Ok(SigningKey::from_bytes(&key_bytes))
    }
    
    /// Sign a request body and return signature headers
    fn sign_request(&self, body: &[u8]) -> (String, String) {
        let signature: Signature = self.signing_key.sign(body);

        (
            self.builder_id.to_string(),
            base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
        )
    }
    
    /// Send heartbeat and metrics to server
    pub async fn send_heartbeat(&self, metrics: &ReportMetricsRequest) -> Result<()> {
        let url = format!(
            "{}/api/v1/builders/{}/heartbeat",
            self.server_url, self.builder_id
        );
        let body = serde_json::to_vec(metrics)?;
        let (builder_id, signature) = self.sign_request(&body);
        
        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Builder-ID", builder_id)
            .header("X-Signature", signature)
            .body(body)
            .send()
            .await
            .context("Failed to send heartbeat")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!("Heartbeat failed with status {}: {}", status, error_text);
        }
        
        debug!("Heartbeat sent successfully");
        Ok(())
    }
    
    /// Get the next available job from the server
    pub async fn get_next_job(&self) -> Result<Option<BuildJob>> {
        let url = format!(
            "{}/api/v1/builders/{}/next-job",
            self.server_url, self.builder_id
        );
        let body = Vec::new(); // Empty body for GET
        let (builder_id, signature) = self.sign_request(&body);
        
        let response = self.client
            .get(&url)
            .header("X-Builder-ID", builder_id)
            .header("X-Signature", signature)
            .send()
            .await
            .context("Failed to request next job")?;
        
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            // No jobs available
            return Ok(None);
        }
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!("Get next job failed with status {}: {}", status, error_text);
        }
        
        let job: BuildJob = response.json().await
            .context("Failed to parse job response")?;
        
        Ok(Some(job))
    }
    
    /// Start a job (mark it as in-progress)
    pub async fn start_job(&self, job_id: uuid::Uuid) -> Result<()> {
        let url = format!(
            "{}/api/v1/builders/{}/jobs/{}/start",
            self.server_url, self.builder_id, job_id
        );
        let body = Vec::new();
        let (builder_id, signature) = self.sign_request(&body);
        
        let response = self.client
            .post(&url)
            .header("X-Builder-ID", builder_id)
            .header("X-Signature", signature)
            .send()
            .await
            .context("Failed to start job")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "unknown error".to_string());
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
        
        let url = format!(
            "{}/api/v1/builders/{}/jobs/{}/complete",
            self.server_url, self.builder_id, job_id
        );
        let request = CompleteRequest {
            output_path: output_path.to_string(),
        };
        let body = serde_json::to_vec(&request)?;
        let (builder_id, signature) = self.sign_request(&body);
        
        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Builder-ID", builder_id)
            .header("X-Signature", signature)
            .body(body)
            .send()
            .await
            .context("Failed to complete job")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "unknown error".to_string());
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
        
        let url = format!(
            "{}/api/v1/builders/{}/jobs/{}/fail",
            self.server_url, self.builder_id, job_id
        );
        let request = FailRequest {
            error_message: error_message.to_string(),
        };
        let body = serde_json::to_vec(&request)?;
        let (builder_id, signature) = self.sign_request(&body);
        
        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Builder-ID", builder_id)
            .header("X-Signature", signature)
            .body(body)
            .send()
            .await
            .context("Failed to fail job")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "unknown error".to_string());
            anyhow::bail!("Fail job request failed with status {}: {}", status, error_text);
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
        
        let url = format!(
            "{}/api/v1/builders/{}/jobs/{}/logs",
            self.server_url, self.builder_id, job_id
        );
        let request = LogRequest {
            logs: log_lines.to_string(),
        };
        let body = serde_json::to_vec(&request)?;
        let (builder_id, signature) = self.sign_request(&body);
        
        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Builder-ID", builder_id)
            .header("X-Signature", signature)
            .body(body)
            .send()
            .await
            .context("Failed to append logs")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "unknown error".to_string());
            warn!("Append logs failed with status {}: {}", status, error_text);
            // Don't fail the entire operation if log append fails
        }
        
        debug!("Logs appended to job {}", job_id);
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
        
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(&key_bytes).unwrap();
        temp_file.flush().unwrap();
        
        let loaded_key = BuilderApiClient::load_private_key(temp_file.path()).unwrap();
        assert_eq!(loaded_key.to_bytes(), key_bytes);
    }
    
    #[test]
    fn test_load_invalid_key_length() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(&[0u8; 16]).unwrap(); // Wrong length
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
        let (id, sig) = client.sign_request(body);
        
        assert_eq!(id, builder_id.to_string());
        assert_eq!(sig.len(), 128); // 64 bytes = 128 hex chars
    }
}
