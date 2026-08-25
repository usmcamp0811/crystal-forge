//! Systems adapter — API fetch with runtime-safe empty-state handling.
//!
//! # Behaviour
//!
//! | Outcome               | Result                                      |
//! |-----------------------|---------------------------------------------|
//! | API returns 2xx       | Real data, no notice                        |
//! | API returns 401/403   | `redirect_to_login: true`                   |
//! | API 5xx / network err | Empty list/detail + notice shown            |
//! | Empty list from API   | Empty `items` vec                           |
//!
//! Views MUST NOT call [`crate::api::client`] directly.
//! All HTTP interactions go through the functions in this module.

use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::api::client::{
    ApiClientError, create_system, deactivate_system, deploy_system, fetch_flake_timelines,
    fetch_flakes, fetch_system, fetch_system_agent_events, fetch_system_commits,
    fetch_system_generations, fetch_system_history, fetch_systems, get_system_deployment_progress,
    update_system, update_system_public_key,
};
use crate::api::models::{
    CreateSystemRequest, CveSummary, DeploySystemRequest, DeploymentStatus, FlakeRegistryItem,
    HealthStatus, PaginatedResponse, PipelineStage, SystemAgentEvent, SystemCommitsResponse,
    SystemDeploymentProgress, SystemDetail, SystemGeneration, SystemHardwareInfo,
    SystemHistoryEntry, SystemNetworkInfo, SystemSecurityInfo, SystemSummary, SystemsListParams,
    UpdateSystemPublicKeyRequest, UpdateSystemRequest,
};

// ─────────────────────────────────────────────────────────────────────────────
// Result Types
// ─────────────────────────────────────────────────────────────────────────────

/// Result of loading the systems list.
#[derive(Debug, Clone)]
pub struct SystemsLoadResult {
    /// The systems to display. Real on success, empty on error or empty API response.
    pub systems: Vec<SystemSummary>,
    /// Human-readable notice shown when list loading fails.
    pub notice: Option<String>,
    /// True when the API returned 401/403 — view should redirect to login.
    pub redirect_to_login: bool,
}

/// Result of loading a single system's detail.
#[derive(Debug, Clone)]
pub struct SystemDetailLoadResult {
    /// The system detail. Real on success, `None` on failure.
    pub system: Option<SystemDetail>,
    /// Human-readable notice shown when detail loading fails.
    pub notice: Option<String>,
    /// True when the API returned 401/403 — view should redirect to login.
    pub redirect_to_login: bool,
}

/// Failure from the public-key mutation endpoint.
///
/// A 5xx response, network failure, or malformed success response is
/// ambiguous because the server may have persisted the key before the
/// response failed (for example, when the subsequent audit insert fails).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemPublicKeyUpdateError {
    /// The server definitively rejected the request before reporting success.
    Rejected(String),
    /// The request may have changed the key and must be reconciled with GET.
    Ambiguous(String),
}

impl std::fmt::Display for SystemPublicKeyUpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(message) | Self::Ambiguous(message) => f.write_str(message),
        }
    }
}

/// Final result of reconciling a public-key mutation with canonical detail.
#[derive(Debug, Clone)]
pub enum SystemPublicKeyRotationOutcome {
    /// The server acknowledged the mutation and canonical state confirms it.
    Confirmed(SystemDetail),
    /// Canonical state confirms the key is active, but the mutation response was
    /// ambiguous, so server-side completion (including audit persistence) is unknown.
    ConfirmedAfterAmbiguousResponse {
        detail: SystemDetail,
        warning: String,
    },
}

/// Failure to establish the requested public-key state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemPublicKeyRotationError {
    /// The mutation was definitively rejected.
    Rejected(String),
    /// The mutation may have committed, but canonical state could not confirm it.
    Unknown(String),
}

impl SystemPublicKeyRotationError {
    pub fn outcome_unknown(&self) -> bool {
        matches!(self, Self::Unknown(_))
    }
}

impl std::fmt::Display for SystemPublicKeyRotationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(message) | Self::Unknown(message) => f.write_str(message),
        }
    }
}

/// Result of loading registered flake names for form dropdowns.
#[derive(Debug, Clone)]
pub struct FlakeNamesLoadResult {
    pub names: Vec<String>,
    pub notice: Option<String>,
    pub redirect_to_login: bool,
}

/// Result of loading flake context for system list/card display.
///
/// Each tuple is `(flake_id, name, branch, latest_commit)`.
#[derive(Debug, Clone)]
pub struct FlakeContextLoadResult {
    pub flakes: Vec<(i32, String, String, Option<String>)>,
    pub notice: Option<String>,
    pub redirect_to_login: bool,
}

#[derive(Debug, Clone)]
pub struct SystemHistoryLoadResult {
    pub entries: Vec<SystemHistoryEntry>,
    pub notice: Option<String>,
    pub redirect_to_login: bool,
}

#[derive(Debug, Clone)]
pub struct SystemAgentEventsLoadResult {
    pub entries: Vec<SystemAgentEvent>,
    pub notice: Option<String>,
    pub redirect_to_login: bool,
}

#[derive(Debug, Clone)]
pub struct SystemGenerationsLoadResult {
    pub generations: Vec<SystemGeneration>,
    pub current_generation: Option<i32>,
    pub notice: Option<String>,
    pub redirect_to_login: bool,
}

#[derive(Debug, Clone)]
pub struct SystemDeploymentProgressLoadResult {
    pub progress: Option<SystemDeploymentProgress>,
    pub notice: Option<String>,
    pub redirect_to_login: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public Adapter Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Fetch the systems list from the backend.
pub async fn load_systems_with_fallback(params: &SystemsListParams) -> SystemsLoadResult {
    match fetch_systems(params).await {
        Ok(PaginatedResponse { items, .. }) => SystemsLoadResult {
            systems: items,
            notice: None,
            redirect_to_login: false,
        },
        Err(error) if should_redirect_to_login(&error) => SystemsLoadResult {
            systems: vec![],
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => SystemsLoadResult {
            systems: vec![],
            notice: Some(format!("Systems API unavailable: {error}")),
            redirect_to_login: false,
        },
    }
}

/// Fetch a single system's detail from the backend.
pub async fn load_system_detail_with_fallback(id: &str) -> SystemDetailLoadResult {
    let uuid = match Uuid::parse_str(id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return SystemDetailLoadResult {
                system: None,
                notice: Some("Unrecognized system ID format".to_string()),
                redirect_to_login: false,
            };
        }
    };

    match fetch_system(&uuid).await {
        Ok(detail) => SystemDetailLoadResult {
            system: Some(detail),
            notice: None,
            redirect_to_login: false,
        },
        Err(error) if should_redirect_to_login(&error) => SystemDetailLoadResult {
            system: None,
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => SystemDetailLoadResult {
            system: None,
            notice: Some(format!("Systems API unavailable: {error}")),
            redirect_to_login: false,
        },
    }
}

/// Fetch flake names for forms.
pub async fn load_flake_names_with_fallback() -> FlakeNamesLoadResult {
    match fetch_flakes().await {
        Ok(flakes) => {
            let mut names = flakes.into_iter().map(|f| f.name).collect::<Vec<_>>();
            names.sort();
            names.dedup();
            FlakeNamesLoadResult {
                names,
                notice: None,
                redirect_to_login: false,
            }
        }
        Err(error) if should_redirect_to_login(&error) => FlakeNamesLoadResult {
            names: vec![],
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => FlakeNamesLoadResult {
            names: vec![],
            notice: Some(format!("Flakes API unavailable: {error}")),
            redirect_to_login: false,
        },
    }
}

/// Fetch flake metadata for UI display (name + latest commit).
pub async fn load_flake_context_with_fallback() -> FlakeContextLoadResult {
    match fetch_flakes().await {
        Ok(flakes) => {
            let timeline_hashes = fetch_flake_timelines()
                .await
                .ok()
                .map(|timelines| {
                    let mut out = std::collections::HashMap::new();
                    for timeline in timelines {
                        let latest = timeline
                            .commits
                            .iter()
                            .find(|c| c.commits_behind == 0)
                            .or_else(|| timeline.commits.first())
                            .map(|c| c.hash.clone());
                        out.insert(timeline.flake_id, latest);
                    }
                    out
                })
                .unwrap_or_default();

            let mapped = flakes
                .into_iter()
                .map(|f: FlakeRegistryItem| {
                    let latest = timeline_hashes.get(&f.id).cloned().flatten();
                    (f.id, f.name, f.branch, latest)
                })
                .collect();

            FlakeContextLoadResult {
                flakes: mapped,
                notice: None,
                redirect_to_login: false,
            }
        }
        Err(error) if should_redirect_to_login(&error) => FlakeContextLoadResult {
            flakes: vec![],
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => FlakeContextLoadResult {
            flakes: vec![],
            notice: Some(format!("Flakes API unavailable: {error}")),
            redirect_to_login: false,
        },
    }
}

/// Create a new system via the backend API.
pub async fn create_system_via_api(
    hostname: String,
    system_configuration_name: Option<String>,
    public_key: String,
    environment: Option<String>,
    flake_name: Option<String>,
    deployment_policy: String,
) -> Result<SystemDetail, String> {
    let request = CreateSystemRequest {
        hostname,
        system_configuration_name,
        public_key,
        environment,
        flake_name,
        deployment_policy,
    };

    match create_system(&request).await {
        Ok(detail) => Ok(detail),
        Err(ApiClientError::Status {
            code: 401 | 403, ..
        }) => Err("Authentication required. Please log in.".to_string()),
        Err(ApiClientError::Status { body, .. }) => Err(body),
        Err(ApiClientError::Network(msg)) => Err(format!("Network error: {}", msg)),
        Err(ApiClientError::Deserialize(msg)) => Err(format!("Invalid response: {}", msg)),
    }
}

pub async fn update_system_via_api(
    system_id: Uuid,
    request: UpdateSystemRequest,
) -> Result<SystemDetail, String> {
    match update_system(&system_id, &request).await {
        Ok(detail) => Ok(detail),
        Err(ApiClientError::Status {
            code: 401 | 403, ..
        }) => Err("Authentication required. Please log in.".to_string()),
        Err(ApiClientError::Status { body, .. }) => Err(body),
        Err(ApiClientError::Network(msg)) => Err(format!("Network error: {}", msg)),
        Err(ApiClientError::Deserialize(msg)) => Err(format!("Invalid response: {}", msg)),
    }
}

/// Update a system's public key via the backend API.
pub async fn update_system_public_key_via_api(
    system_id: Uuid,
    new_public_key: String,
) -> Result<String, SystemPublicKeyUpdateError> {
    let request = UpdateSystemPublicKeyRequest {
        public_key: new_public_key,
    };

    update_system_public_key(&system_id, &request)
        .await
        .map(|response| response.message)
        .map_err(classify_public_key_update_error)
}

fn classify_public_key_update_error(error: ApiClientError) -> SystemPublicKeyUpdateError {
    match error {
        ApiClientError::Status {
            code: 401 | 403, ..
        } => SystemPublicKeyUpdateError::Rejected(
            "Authentication required. Please log in.".to_string(),
        ),
        ApiClientError::Status { code, body } if code >= 500 => {
            SystemPublicKeyUpdateError::Ambiguous(body)
        }
        ApiClientError::Status { body, .. } => SystemPublicKeyUpdateError::Rejected(body),
        ApiClientError::Network(msg) => {
            SystemPublicKeyUpdateError::Ambiguous(format!("Network error: {msg}"))
        }
        ApiClientError::Deserialize(msg) => {
            SystemPublicKeyUpdateError::Ambiguous(format!("Invalid response: {msg}"))
        }
    }
}

/// Update a system public key and reconcile the outcome against canonical detail.
///
/// The server can return 500 after persisting the key if its audit write fails.
/// A successful response can also be lost to the network. Therefore all 2xx and
/// ambiguous outcomes are followed by GET, and only a matching server-derived
/// fingerprint is reported as success.
pub async fn update_system_public_key_and_reconcile(
    system_id: Uuid,
    new_public_key: String,
    expected_fingerprint: String,
) -> Result<SystemPublicKeyRotationOutcome, SystemPublicKeyRotationError> {
    let update_result = update_system_public_key_via_api(system_id, new_public_key).await;

    if let Err(SystemPublicKeyUpdateError::Rejected(message)) = &update_result {
        return Err(SystemPublicKeyRotationError::Rejected(message.clone()));
    }

    let detail_result = load_system_detail_with_fallback(&system_id.to_string()).await;
    if let Some(detail) = detail_result.system
        && detail.public_key_fingerprint.as_deref() == Some(expected_fingerprint.as_str())
    {
        return Ok(match update_result {
            Ok(_) => SystemPublicKeyRotationOutcome::Confirmed(detail),
            Err(SystemPublicKeyUpdateError::Ambiguous(message)) => {
                SystemPublicKeyRotationOutcome::ConfirmedAfterAmbiguousResponse {
                    detail,
                    warning: format!(
                        "The replacement key is confirmed active, but the rotation request returned an error after the key changed: {message}. Server-side completion, including the audit event, could not be confirmed."
                    ),
                }
            }
            Err(SystemPublicKeyUpdateError::Rejected(message)) => {
                return Err(SystemPublicKeyRotationError::Rejected(message));
            }
        });
    }

    let mutation_context = match update_result {
        Ok(_) => "The server acknowledged the request, but the current key could not be confirmed."
            .to_string(),
        Err(error) => format!("{error}. The current key could not be confirmed."),
    };
    let read_context = detail_result
        .notice
        .map(|notice| format!(" Detail refresh failed: {notice}."))
        .unwrap_or_default();

    Err(SystemPublicKeyRotationError::Unknown(format!(
        "{mutation_context}{read_context} Keep the replacement credential and verify the system state before retrying."
    )))
}

/// Deploy a system to a specific commit via the backend API.
pub async fn deploy_system_via_api(system_id: Uuid, commit_sha: String) -> Result<String, String> {
    let request = DeploySystemRequest { commit_sha };

    match deploy_system(&system_id, &request).await {
        Ok(response) => Ok(response.message),
        Err(ApiClientError::Status {
            code: 401 | 403, ..
        }) => Err("Authentication required. Please log in.".to_string()),
        Err(ApiClientError::Status { body, .. }) => Err(body),
        Err(ApiClientError::Network(msg)) => Err(format!("Network error: {}", msg)),
        Err(ApiClientError::Deserialize(msg)) => Err(format!("Invalid response: {}", msg)),
    }
}

/// Fetch available commits for a system from the backend API.
pub async fn fetch_system_commits_via_api(
    system_id: Uuid,
) -> Result<SystemCommitsResponse, String> {
    match fetch_system_commits(&system_id).await {
        Ok(response) => Ok(response),
        Err(ApiClientError::Status {
            code: 401 | 403, ..
        }) => Err("Authentication required. Please log in.".to_string()),
        Err(ApiClientError::Status { body, .. }) => Err(body),
        Err(ApiClientError::Network(msg)) => Err(format!("Network error: {}", msg)),
        Err(ApiClientError::Deserialize(msg)) => Err(format!("Invalid response: {}", msg)),
    }
}

pub async fn load_system_history_with_fallback(system_id: Uuid) -> SystemHistoryLoadResult {
    match fetch_system_history(&system_id).await {
        Ok(entries) => SystemHistoryLoadResult {
            entries,
            notice: None,
            redirect_to_login: false,
        },
        Err(error) if should_redirect_to_login(&error) => SystemHistoryLoadResult {
            entries: vec![],
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => SystemHistoryLoadResult {
            entries: vec![],
            notice: Some(format!("History API unavailable: {error}")),
            redirect_to_login: false,
        },
    }
}

pub async fn load_system_deployment_progress_with_fallback(
    system_id: Uuid,
) -> SystemDeploymentProgressLoadResult {
    match get_system_deployment_progress(&system_id).await {
        Ok(progress) => SystemDeploymentProgressLoadResult {
            progress,
            notice: None,
            redirect_to_login: false,
        },
        Err(error) if should_redirect_to_login(&error) => SystemDeploymentProgressLoadResult {
            progress: None,
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => SystemDeploymentProgressLoadResult {
            progress: None,
            notice: Some(format!("Deployment status API unavailable: {error}")),
            redirect_to_login: false,
        },
    }
}

pub async fn load_system_agent_events_with_fallback(
    system_id: Uuid,
) -> SystemAgentEventsLoadResult {
    match fetch_system_agent_events(&system_id).await {
        Ok(entries) => SystemAgentEventsLoadResult {
            entries,
            notice: None,
            redirect_to_login: false,
        },
        Err(error) if should_redirect_to_login(&error) => SystemAgentEventsLoadResult {
            entries: vec![],
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => SystemAgentEventsLoadResult {
            entries: vec![],
            notice: Some(format!("Agent events API unavailable: {error}")),
            redirect_to_login: false,
        },
    }
}

pub async fn load_system_generations_with_fallback(system_id: Uuid) -> SystemGenerationsLoadResult {
    match fetch_system_generations(&system_id).await {
        Ok(response) => SystemGenerationsLoadResult {
            generations: response.generations,
            current_generation: response.current_generation,
            notice: None,
            redirect_to_login: false,
        },
        Err(error) if should_redirect_to_login(&error) => SystemGenerationsLoadResult {
            generations: vec![],
            current_generation: None,
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => SystemGenerationsLoadResult {
            generations: vec![],
            current_generation: None,
            notice: Some(format!("Generations API unavailable: {error}")),
            redirect_to_login: false,
        },
    }
}

/// Disable (soft-delete) a system via the backend API.
pub async fn deactivate_system_via_api(system_id: Uuid) -> Result<String, String> {
    match deactivate_system(&system_id).await {
        Ok(response) => Ok(response.message),
        Err(ApiClientError::Status {
            code: 401 | 403, ..
        }) => Err("Authentication required. Please log in.".to_string()),
        Err(ApiClientError::Status { body, .. }) => Err(body),
        Err(ApiClientError::Network(msg)) => Err(format!("Network error: {}", msg)),
        Err(ApiClientError::Deserialize(msg)) => Err(format!("Invalid response: {}", msg)),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fallback Data
// ─────────────────────────────────────────────────────────────────────────────

/// Deterministic fallback list of systems used when the API is unavailable.
///
/// Timestamps are anchored to a fixed point so the data is stable across renders.
pub fn fallback_systems() -> Vec<SystemSummary> {
    let now = deterministic_fallback_timestamp();
    vec![
        SystemSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            hostname: "atlas-01".to_string(),
            system_configuration_name: None,
            environment: Some("production".to_string()),
            flake_id: Some(1),
            primary_ip: Some("10.0.1.10".to_string()),
            health_status: HealthStatus::Healthy,
            deployment_status: DeploymentStatus::UpToDate,
            pipeline_stage: Some(PipelineStage::BuildComplete),
            cve_counts: CveSummary {
                critical: 0,
                high: 2,
                medium: 5,
                low: 12,
            },
            nixos_version: Some("24.11".to_string()),
            last_seen: Some(now - Duration::minutes(5)),
            deployment_policy: "auto_latest".to_string(),
            fqdn: None,
            heartbeat_interval_secs: None,
            boot_id: None,
            effective_heartbeat_interval_secs: 600,
        },
        SystemSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
            hostname: "atlas-02".to_string(),
            system_configuration_name: None,
            environment: Some("production".to_string()),
            flake_id: Some(1),
            primary_ip: Some("10.0.1.11".to_string()),
            health_status: HealthStatus::Healthy,
            deployment_status: DeploymentStatus::Behind,
            pipeline_stage: Some(PipelineStage::ReadyForDeploy),
            cve_counts: CveSummary {
                critical: 1,
                high: 3,
                medium: 8,
                low: 15,
            },
            nixos_version: Some("24.11".to_string()),
            last_seen: Some(now - Duration::minutes(10)),
            deployment_policy: "manual".to_string(),
            fqdn: None,
            heartbeat_interval_secs: None,
            boot_id: None,
            effective_heartbeat_interval_secs: 600,
        },
        SystemSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap(),
            hostname: "staging-01".to_string(),
            system_configuration_name: None,
            environment: Some("staging".to_string()),
            flake_id: Some(2),
            primary_ip: Some("10.0.2.10".to_string()),
            health_status: HealthStatus::Warning,
            deployment_status: DeploymentStatus::Behind,
            pipeline_stage: Some(PipelineStage::ReadyForDeploy),
            cve_counts: CveSummary {
                critical: 0,
                high: 0,
                medium: 2,
                low: 5,
            },
            nixos_version: Some("24.11".to_string()),
            last_seen: Some(now - Duration::hours(1)),
            deployment_policy: "manual".to_string(),
            fqdn: None,
            heartbeat_interval_secs: None,
            boot_id: None,
            effective_heartbeat_interval_secs: 600,
        },
        SystemSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000004").unwrap(),
            hostname: "dev-box".to_string(),
            system_configuration_name: None,
            environment: Some("development".to_string()),
            flake_id: None,
            primary_ip: Some("10.0.3.20".to_string()),
            health_status: HealthStatus::Offline,
            deployment_status: DeploymentStatus::NeverDeployed,
            pipeline_stage: Some(PipelineStage::ReadyForBuild),
            cve_counts: CveSummary {
                critical: 0,
                high: 0,
                medium: 0,
                low: 0,
            },
            nixos_version: None,
            last_seen: Some(now - Duration::days(3)),
            deployment_policy: "manual".to_string(),
            fqdn: None,
            heartbeat_interval_secs: None,
            boot_id: None,
            effective_heartbeat_interval_secs: 600,
        },
    ]
}

pub fn fallback_flake_names() -> Vec<String> {
    vec![
        "infrastructure".to_string(),
        "workstations".to_string(),
        "edge-nodes".to_string(),
    ]
}

/// Fallback system detail used when no real data and no mock for the given ID.
pub fn fallback_system_detail() -> SystemDetail {
    let now = deterministic_fallback_timestamp();
    SystemDetail {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap(),
        hostname: "unknown-system".to_string(),
        fqdn: None,
        system_configuration_name: None,
        environment: None,
        is_active: false,
        deployment_policy: "manual".to_string(),
        health_status: HealthStatus::Offline,
        deployment_status: DeploymentStatus::Unknown,
        pipeline_stage: Some(PipelineStage::Unknown),
        nixos_version: None,
        kernel: None,
        agent_version: None,
        current_store_path: None,
        generation: None,
        generation_matches_current_store_path: None,
        hardware: SystemHardwareInfo {
            cpu_brand: None,
            cpu_cores: None,
            memory_gb: None,
            uptime_secs: None,
            board_serial: None,
            bios_version: None,
        },
        network: SystemNetworkInfo {
            primary_ip: None,
            primary_mac: None,
            gateway_ip: None,
            reachability: "direct".to_string(),
        },
        security: SystemSecurityInfo {
            tpm_present: None,
            secure_boot_enabled: None,
            fips_mode: None,
            selinux_status: None,
        },
        cve_counts: CveSummary {
            critical: 0,
            high: 0,
            medium: 0,
            low: 0,
        },
        flake: None,
        last_seen: None,
        created_at: now,
        updated_at: now,
        heartbeat_interval_secs: None,
        effective_heartbeat_interval_secs: 600,
        boot_id: None,
        restart_type: None,
        last_restart_at: None,
        public_key_fingerprint: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn should_redirect_to_login(error: &ApiClientError) -> bool {
    matches!(
        error,
        ApiClientError::Status {
            code: 401 | 403,
            ..
        }
    )
}

/// Fixed timestamp used for deterministic fallback data so renders are stable.
fn deterministic_fallback_timestamp() -> chrono::DateTime<Utc> {
    use chrono::TimeZone;
    Utc.with_ymd_and_hms(2025, 1, 15, 12, 0, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_redirect_for_auth_errors() {
        assert!(should_redirect_to_login(&ApiClientError::Status {
            code: 401,
            body: "unauthorized".to_string(),
        }));
        assert!(should_redirect_to_login(&ApiClientError::Status {
            code: 403,
            body: "forbidden".to_string(),
        }));
    }

    #[test]
    fn should_not_redirect_for_server_or_network_errors() {
        assert!(!should_redirect_to_login(&ApiClientError::Status {
            code: 500,
            body: "internal server error".to_string(),
        }));
        assert!(!should_redirect_to_login(&ApiClientError::Network(
            "connection refused".to_string()
        )));
    }

    #[test]
    fn public_key_update_classifies_rejections_and_unknown_outcomes() {
        assert!(matches!(
            classify_public_key_update_error(ApiClientError::Status {
                code: 400,
                body: "invalid key".to_string(),
            }),
            SystemPublicKeyUpdateError::Rejected(_)
        ));
        assert!(matches!(
            classify_public_key_update_error(ApiClientError::Status {
                code: 500,
                body: "audit write failed".to_string(),
            }),
            SystemPublicKeyUpdateError::Ambiguous(_)
        ));
        assert!(matches!(
            classify_public_key_update_error(ApiClientError::Network(
                "connection reset".to_string()
            )),
            SystemPublicKeyUpdateError::Ambiguous(_)
        ));
        assert!(matches!(
            classify_public_key_update_error(ApiClientError::Deserialize(
                "invalid success response".to_string()
            )),
            SystemPublicKeyUpdateError::Ambiguous(_)
        ));
    }

    #[test]
    fn fallback_systems_is_deterministic() {
        let a = fallback_systems();
        let b = fallback_systems();
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.id, y.id);
            assert_eq!(x.hostname, y.hostname);
            assert_eq!(x.last_seen, y.last_seen);
        }
    }

    #[test]
    fn fallback_systems_has_expected_entries() {
        let systems = fallback_systems();
        assert_eq!(systems.len(), 4);
        assert_eq!(systems[0].hostname, "atlas-01");
        assert_eq!(systems[1].hostname, "atlas-02");
        assert_eq!(systems[2].hostname, "staging-01");
        assert_eq!(systems[3].hostname, "dev-box");
    }
}
