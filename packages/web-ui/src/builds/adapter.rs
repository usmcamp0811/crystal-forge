//! Builds adapter — API fetch with deterministic fallback.
//!
//! # Behaviour
//!
//! | Outcome               | Result                                      |
//! |-----------------------|---------------------------------------------|
//! | API returns 2xx       | Real data, no notice                        |
//! | API returns 401/403   | `redirect_to_login: true`                   |
//! | API 5xx / network err | Fallback mock data, notice shown            |
//! | Empty list from API   | Empty `items` vec (not fallback)            |
//!
//! Views MUST NOT call [`crate::api::client`] directly.
//! All HTTP interactions go through the functions in this module.

use chrono::Utc;

use crate::api::client::{ApiClientError, fetch_builds};
use crate::api::models::BuildStatus as ApiModelBuildStatus;
use crate::components::builds::{BuildItem, BuildStatus, WorkerItem, WorkerStatus};

// ─────────────────────────────────────────────────────────────────────────────
// Owned Bridge Types
// ─────────────────────────────────────────────────────────────────────────────

/// Owned version of [`BuildItem`] for API data.
///
/// The UI component tree expects `BuildItem` with `&'static str` fields.
/// This struct holds owned data returned from the API, which the view converts
/// into the UI type via [`owned_to_build_item`].
#[derive(Debug, Clone)]
pub struct OwnedBuildItem {
    pub id: i32,
    pub hostname: String,
    pub flake: String,
    pub commit: String,
    pub branch: String,
    pub worker_id: String,
    pub queued_for: String,
    pub runtime: Option<String>,
    pub started_by: String,
    pub status: BuildStatus,
    pub summary: String,
}

/// Convert an [`OwnedBuildItem`] into a [`BuildItem`] by leaking the strings.
///
/// This is safe in a single-page WASM app: leaked strings live for the process
/// lifetime, which is the same as the page lifetime. We accept the tiny memory
/// overhead because build queue items are few and infrequent.
pub fn owned_to_build_item(item: OwnedBuildItem) -> BuildItem {
    BuildItem {
        id: item.id,
        hostname: Box::leak(item.hostname.into_boxed_str()),
        flake: Box::leak(item.flake.into_boxed_str()),
        commit: Box::leak(item.commit.into_boxed_str()),
        branch: Box::leak(item.branch.into_boxed_str()),
        worker_id: Box::leak(item.worker_id.into_boxed_str()),
        queued_for: Box::leak(item.queued_for.into_boxed_str()),
        runtime: item.runtime.map(|r| Box::leak(r.into_boxed_str()) as &'static str),
        started_by: Box::leak(item.started_by.into_boxed_str()),
        status: item.status,
        summary: Box::leak(item.summary.into_boxed_str()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Result Types
// ─────────────────────────────────────────────────────────────────────────────

/// Result of loading the builds queue.
#[derive(Debug, Clone)]
pub struct BuildsLoadResult {
    /// Build items to display (real data, fallback, or empty).
    pub builds: Vec<OwnedBuildItem>,
    /// Human-readable notice shown when using fallback data.
    pub notice: Option<String>,
    /// True when the API returned 401/403 — view should redirect to login.
    pub redirect_to_login: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public Adapter Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Fetch the build queue from the backend, with fallback to deterministic mock data.
pub async fn load_builds_with_fallback() -> BuildsLoadResult {
    match fetch_builds().await {
        Ok(summary) => {
            let builds = summary
                .items
                .into_iter()
                .enumerate()
                .map(|(idx, item)| api_item_to_owned(idx as i32 + 1, item))
                .collect();

            BuildsLoadResult {
                builds,
                notice: None,
                redirect_to_login: false,
            }
        }
        Err(error) if should_redirect_to_login(&error) => BuildsLoadResult {
            builds: fallback_builds(),
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => BuildsLoadResult {
            builds: fallback_builds(),
            notice: Some(format!(
                "Builds API unavailable, using deterministic fallback data: {error}"
            )),
            redirect_to_login: false,
        },
    }
}

/// Deterministic fallback build list for when the API is unavailable.
///
/// Returns the same data as the previous mock so no behaviour changes when
/// the backend is down.
pub fn fallback_builds() -> Vec<OwnedBuildItem> {
    vec![
        OwnedBuildItem {
            id: 1,
            hostname: "atlas-01".to_string(),
            flake: "campground".to_string(),
            commit: "a38f45fba91d4b0a5d80840c09b0910c70fa013e".to_string(),
            branch: "main".to_string(),
            worker_id: "worker-a".to_string(),
            queued_for: "queued 00:58 ago".to_string(),
            runtime: Some("02:13".to_string()),
            started_by: "scheduler".to_string(),
            status: BuildStatus::Building,
            summary: "nix build .#nixosConfigurations.atlas-01.config.system.build.toplevel"
                .to_string(),
        },
        OwnedBuildItem {
            id: 2,
            hostname: "luna-02".to_string(),
            flake: "campground".to_string(),
            commit: "75c2fbf719ac2654af9f1dc4b773f502f9db515e".to_string(),
            branch: "main".to_string(),
            worker_id: "worker-b".to_string(),
            queued_for: "queued 01:32 ago".to_string(),
            runtime: None,
            started_by: "scheduler".to_string(),
            status: BuildStatus::Queued,
            summary: "waiting for free worker slot".to_string(),
        },
    ]
}

/// Deterministic fallback worker list for when the API is unavailable.
///
/// Workers are not yet exposed via an API endpoint; this always returns the
/// deterministic fallback.
pub fn fallback_workers() -> Vec<WorkerItem> {
    vec![
        WorkerItem {
            id: "worker-a",
            name: "worker-a",
            active_slots: 1,
            total_slots: 4,
            queue_depth: 2,
            status: WorkerStatus::Running,
        },
        WorkerItem {
            id: "worker-b",
            name: "worker-b",
            active_slots: 0,
            total_slots: 4,
            queue_depth: 0,
            status: WorkerStatus::Running,
        },
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn api_item_to_owned(id: i32, item: crate::api::models::BuildQueueItem) -> OwnedBuildItem {
    let status = match item.status {
        ApiModelBuildStatus::Building => BuildStatus::Building,
        ApiModelBuildStatus::Queued => BuildStatus::Queued,
        ApiModelBuildStatus::Failed => BuildStatus::Failed,
        ApiModelBuildStatus::Complete => BuildStatus::Complete,
        ApiModelBuildStatus::Idle => BuildStatus::Queued,
    };

    let queued_age = format_age(item.queued_at);
    let runtime = item.elapsed_secs.map(format_elapsed);

    // Derive worker_id from the list of workers — not available per-item in the
    // current API. Use a placeholder that the live build engine would populate.
    let worker_id = "worker-a".to_string();

    let summary = format!(
        "nix build .#nixosConfigurations.{}.config.system.build.toplevel",
        item.hostname
    );

    OwnedBuildItem {
        id,
        hostname: item.hostname,
        flake: item.flake_name,
        commit: item.commit_hash,
        branch: "main".to_string(),
        worker_id,
        queued_for: queued_age,
        runtime,
        started_by: "scheduler".to_string(),
        status,
        summary,
    }
}

fn should_redirect_to_login(error: &ApiClientError) -> bool {
    matches!(
        error,
        ApiClientError::Status { code: 401 | 403, .. }
    )
}

/// Format seconds elapsed as `MM:SS`.
fn format_elapsed(secs: i64) -> String {
    let minutes = secs / 60;
    let seconds = secs % 60;
    format!("{minutes:02}:{seconds:02}")
}

/// Format a `DateTime<Utc>` as a human-readable age string.
fn format_age(ts: chrono::DateTime<Utc>) -> String {
    let age_secs = (Utc::now() - ts).num_seconds().max(0);
    let minutes = age_secs / 60;
    let seconds = age_secs % 60;
    format!("queued {:02}:{:02} ago", minutes, seconds)
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
    fn fallback_builds_is_deterministic() {
        let a = fallback_builds();
        let b = fallback_builds();
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.hostname, y.hostname);
            assert_eq!(x.commit, y.commit);
        }
    }

    #[test]
    fn fallback_builds_has_expected_entries() {
        let builds = fallback_builds();
        assert_eq!(builds.len(), 2);
        assert_eq!(builds[0].hostname, "atlas-01");
        assert_eq!(builds[1].hostname, "luna-02");
    }

    #[test]
    fn format_elapsed_formats_correctly() {
        assert_eq!(format_elapsed(0), "00:00");
        assert_eq!(format_elapsed(65), "01:05");
        assert_eq!(format_elapsed(3600), "60:00");
    }

    #[test]
    fn owned_to_build_item_converts_status() {
        let owned = OwnedBuildItem {
            id: 42,
            hostname: "host".to_string(),
            flake: "flake".to_string(),
            commit: "abc1234".to_string(),
            branch: "main".to_string(),
            worker_id: "worker-x".to_string(),
            queued_for: "queued 00:01 ago".to_string(),
            runtime: Some("01:30".to_string()),
            started_by: "scheduler".to_string(),
            status: BuildStatus::Building,
            summary: "summary".to_string(),
        };

        let item = owned_to_build_item(owned);
        assert_eq!(item.id, 42);
        assert_eq!(item.hostname, "host");
        assert_eq!(item.status, BuildStatus::Building);
        assert_eq!(item.runtime, Some("01:30"));
    }
}
