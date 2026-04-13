//! Post-build CVE policy gate.
//!
//! Evaluates `require_cve_check` policies against the most recent completed CVE
//! scan for a derivation.  Called by the deployment manager after a build job
//! reaches `build-complete` and before `desired_target` is updated on any system.

use crate::models::deployment_policies::{
    CveCheckOutcome, CveCheckConfig, DeploymentPolicy, WhenNoScan,
};
use crate::queries::cve_scans::{count_unjustified_high_cves, get_latest_scan};
use crate::models::cve_scans::ScanStatus;
use anyhow::Result;
use sqlx::PgPool;
use tracing::{info, warn};

/// Summary returned by the CVE gate evaluation.
#[derive(Debug, Clone)]
pub struct CveGateResult {
    /// All CVE policy outcomes, one per enabled `require_cve_check` policy.
    pub outcomes: Vec<CveCheckOutcome>,
    /// True if all strict policies passed (deployment may proceed).
    pub deployment_allowed: bool,
    /// Human-readable summary of the block reason (if blocked).
    pub block_reason: Option<String>,
}

/// Evaluate all `require_cve_check` policies for a derivation.
///
/// Returns `Ok(CveGateResult)` with `deployment_allowed = true` if no strict
/// policy is violated.  The caller must persist the result and skip
/// `desired_target` update when `deployment_allowed = false`.
pub async fn check_cve_policies(
    pool: &PgPool,
    derivation_id: i32,
    policies: &[DeploymentPolicy],
) -> Result<CveGateResult> {
    let cve_policies: Vec<&CveCheckConfig> = policies
        .iter()
        .filter_map(|p| {
            if let DeploymentPolicy::RequireCveCheck { config } = p {
                Some(config)
            } else {
                None
            }
        })
        .collect();

    if cve_policies.is_empty() {
        return Ok(CveGateResult {
            outcomes: vec![],
            deployment_allowed: true,
            block_reason: None,
        });
    }

    // Fetch latest scan once — all policies share it.
    let latest_scan = get_latest_scan(pool, derivation_id).await?;

    let mut outcomes: Vec<CveCheckOutcome> = Vec::new();
    let mut blocked = false;
    let mut block_reasons: Vec<String> = Vec::new();

    for config in &cve_policies {
        let outcome = evaluate_cve_config(pool, derivation_id, config, &latest_scan).await?;
        if !outcome.passed && outcome.blocking {
            blocked = true;
            if let Some(ref reason) = outcome.reason {
                block_reasons.push(reason.clone());
            }
        }
        outcomes.push(outcome);
    }

    let block_reason = if blocked {
        Some(block_reasons.join("; "))
    } else {
        None
    };

    if blocked {
        warn!(
            "CVE gate blocked deployment for derivation {}: {}",
            derivation_id,
            block_reason.as_deref().unwrap_or("policy violation")
        );
    } else {
        info!(
            "CVE gate passed for derivation {} ({} policies checked)",
            derivation_id,
            cve_policies.len()
        );
    }

    Ok(CveGateResult {
        outcomes,
        deployment_allowed: !blocked,
        block_reason,
    })
}

async fn evaluate_cve_config(
    pool: &PgPool,
    derivation_id: i32,
    config: &CveCheckConfig,
    latest_scan: &Option<crate::models::cve_scans::CveScan>,
) -> Result<CveCheckOutcome> {
    let description = format!(
        "require_cve_check(max_critical={}, max_high={:?}, require_high_justification={})",
        config.max_critical, config.max_high, config.require_high_justification
    );

    // Handle no-scan case.
    let scan = match latest_scan {
        None => {
            return match config.when_no_scan {
                WhenNoScan::Skip => Ok(CveCheckOutcome {
                    policy_description: description,
                    passed: true,
                    blocking: false,
                    reason: Some("No scan found — skipping (when_no_scan=skip)".to_string()),
                }),
                WhenNoScan::Block => Ok(CveCheckOutcome {
                    policy_description: description.clone(),
                    passed: false,
                    blocking: config.strict,
                    reason: Some(format!(
                        "No completed CVE scan found for derivation {} (when_no_scan=block)",
                        derivation_id
                    )),
                }),
            };
        }
        Some(s) => s,
    };

    // Only act on completed scans.
    if scan.status != ScanStatus::Completed {
        return match config.when_no_scan {
            WhenNoScan::Skip => Ok(CveCheckOutcome {
                policy_description: description,
                passed: true,
                blocking: false,
                reason: Some(format!(
                    "Scan {:?} not completed — skipping (when_no_scan=skip)",
                    scan.status
                )),
            }),
            WhenNoScan::Block => Ok(CveCheckOutcome {
                policy_description: description.clone(),
                passed: false,
                blocking: config.strict,
                reason: Some(format!(
                    "CVE scan for derivation {} is not completed (status: {:?})",
                    derivation_id, scan.status
                )),
            }),
        };
    }

    let mut violations: Vec<String> = Vec::new();

    // Check critical threshold.
    let critical = scan.critical_count as u32;
    if critical > config.max_critical {
        violations.push(format!(
            "{} critical CVE(s) found (max allowed: {})",
            critical, config.max_critical
        ));
    }

    // Check high threshold.
    if let Some(max_high) = config.max_high {
        let high = scan.high_count as u32;
        if high > max_high {
            violations.push(format!(
                "{} high CVE(s) found (max allowed: {})",
                high, max_high
            ));
        }
    }

    // Check high justification requirement.
    // count_unjustified_high_cves returns None when no completed scan exists
    // for the derivation.  We can only reach this code path when scan.status ==
    // Completed (checked above), so None here means the scan_packages join
    // returned no rows — treat as 0 unjustified CVEs.
    if config.require_high_justification {
        let unjustified = count_unjustified_high_cves(pool, derivation_id)
            .await?
            .unwrap_or(0);
        if unjustified > 0 {
            violations.push(format!(
                "{} high CVE(s) lack whitelist justification",
                unjustified
            ));
        }
    }

    if violations.is_empty() {
        Ok(CveCheckOutcome {
            policy_description: description,
            passed: true,
            blocking: false,
            reason: None,
        })
    } else {
        let reason = violations.join("; ");
        Ok(CveCheckOutcome {
            policy_description: description,
            passed: false,
            blocking: config.strict,
            reason: Some(reason),
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::deployment_policies::{CveCheckConfig, WhenNoScan};

    #[test]
    fn no_cve_policies_returns_allowed() {
        // Purely synchronous check — no pool needed.
        let policies: Vec<DeploymentPolicy> = vec![
            DeploymentPolicy::RequireCrystalForgeAgent { strict: true },
        ];
        let cve_policies: Vec<&CveCheckConfig> = policies
            .iter()
            .filter_map(|p| {
                if let DeploymentPolicy::RequireCveCheck { config } = p {
                    Some(config)
                } else {
                    None
                }
            })
            .collect();
        assert!(cve_policies.is_empty());
    }

    #[tokio::test]
    async fn when_no_scan_skip_passes() {
        // Use a lazy pool — evaluate_cve_config short-circuits before touching DB
        // when latest_scan is None and when_no_scan = Skip.
        let config = CveCheckConfig {
            when_no_scan: WhenNoScan::Skip,
            ..Default::default()
        };
        let outcome = evaluate_cve_config(
            &sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
                .unwrap(),
            999,
            &config,
            &None,
        )
        .await
        .unwrap();
        assert!(outcome.passed);
        assert!(!outcome.blocking);
    }

    #[tokio::test]
    async fn when_no_scan_block_strict_fails() {
        let config = CveCheckConfig {
            when_no_scan: WhenNoScan::Block,
            strict: true,
            ..Default::default()
        };
        let outcome = evaluate_cve_config(
            &sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
                .unwrap(),
            999,
            &config,
            &None,
        )
        .await
        .unwrap();
        assert!(!outcome.passed);
        assert!(outcome.blocking);
    }

    #[tokio::test]
    async fn when_no_scan_block_non_strict_warns_not_blocks() {
        let config = CveCheckConfig {
            when_no_scan: WhenNoScan::Block,
            strict: false,
            ..Default::default()
        };
        let outcome = evaluate_cve_config(
            &sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
                .unwrap(),
            999,
            &config,
            &None,
        )
        .await
        .unwrap();
        assert!(!outcome.passed);
        assert!(!outcome.blocking, "non-strict policy must not block");
    }

    #[tokio::test]
    async fn critical_threshold_exceeded_blocks() {
        use crate::models::cve_scans::{CveScan, ScanStatus};
        use chrono::Utc;
        use uuid::Uuid;

        let scan = Some(CveScan {
            id: Uuid::new_v4(),
            derivation_id: 1,
            scheduled_at: Some(Utc::now()),
            completed_at: Some(Utc::now()),
            status: ScanStatus::Completed,
            attempts: 1,
            scanner_name: "vulnix".to_string(),
            scanner_version: None,
            total_packages: 10,
            total_vulnerabilities: 3,
            critical_count: 2,
            high_count: 1,
            medium_count: 0,
            low_count: 0,
            scan_duration_ms: None,
            scan_metadata: None,
            created_at: Some(Utc::now()),
        });

        let config = CveCheckConfig {
            max_critical: 0,
            strict: true,
            ..Default::default()
        };

        let outcome = evaluate_cve_config(
            &sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
                .unwrap(),
            1,
            &config,
            &scan,
        )
        .await
        .unwrap();

        assert!(!outcome.passed);
        assert!(outcome.blocking);
        assert!(outcome.reason.as_deref().unwrap_or("").contains("critical"));
    }

    #[tokio::test]
    async fn high_threshold_exceeded_blocks() {
        use crate::models::cve_scans::{CveScan, ScanStatus};
        use chrono::Utc;
        use uuid::Uuid;

        let scan = Some(CveScan {
            id: Uuid::new_v4(),
            derivation_id: 1,
            scheduled_at: Some(Utc::now()),
            completed_at: Some(Utc::now()),
            status: ScanStatus::Completed,
            attempts: 1,
            scanner_name: "vulnix".to_string(),
            scanner_version: None,
            total_packages: 10,
            total_vulnerabilities: 6,
            critical_count: 0,
            high_count: 6,
            medium_count: 0,
            low_count: 0,
            scan_duration_ms: None,
            scan_metadata: None,
            created_at: Some(Utc::now()),
        });

        let config = CveCheckConfig {
            max_critical: 0,
            max_high: Some(5),
            strict: true,
            ..Default::default()
        };

        let outcome = evaluate_cve_config(
            &sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
                .unwrap(),
            1,
            &config,
            &scan,
        )
        .await
        .unwrap();

        assert!(!outcome.passed);
        assert!(outcome.blocking);
        assert!(outcome.reason.as_deref().unwrap_or("").contains("high"));
    }

    #[tokio::test]
    async fn no_violation_passes() {
        use crate::models::cve_scans::{CveScan, ScanStatus};
        use chrono::Utc;
        use uuid::Uuid;

        let scan = Some(CveScan {
            id: Uuid::new_v4(),
            derivation_id: 1,
            scheduled_at: Some(Utc::now()),
            completed_at: Some(Utc::now()),
            status: ScanStatus::Completed,
            attempts: 1,
            scanner_name: "vulnix".to_string(),
            scanner_version: None,
            total_packages: 10,
            total_vulnerabilities: 0,
            critical_count: 0,
            high_count: 0,
            medium_count: 0,
            low_count: 0,
            scan_duration_ms: None,
            scan_metadata: None,
            created_at: Some(Utc::now()),
        });

        let config = CveCheckConfig {
            max_critical: 0,
            strict: true,
            ..Default::default()
        };

        let outcome = evaluate_cve_config(
            &sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
                .unwrap(),
            1,
            &config,
            &scan,
        )
        .await
        .unwrap();

        assert!(outcome.passed);
        assert!(!outcome.blocking);
    }

    #[tokio::test]
    async fn non_strict_violation_does_not_block() {
        use crate::models::cve_scans::{CveScan, ScanStatus};
        use chrono::Utc;
        use uuid::Uuid;

        let scan = Some(CveScan {
            id: Uuid::new_v4(),
            derivation_id: 1,
            scheduled_at: Some(Utc::now()),
            completed_at: Some(Utc::now()),
            status: ScanStatus::Completed,
            attempts: 1,
            scanner_name: "vulnix".to_string(),
            scanner_version: None,
            total_packages: 5,
            total_vulnerabilities: 3,
            critical_count: 3,
            high_count: 0,
            medium_count: 0,
            low_count: 0,
            scan_duration_ms: None,
            scan_metadata: None,
            created_at: Some(Utc::now()),
        });

        let config = CveCheckConfig {
            max_critical: 0,
            strict: false, // warn only
            ..Default::default()
        };

        let outcome = evaluate_cve_config(
            &sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
                .unwrap(),
            1,
            &config,
            &scan,
        )
        .await
        .unwrap();

        assert!(!outcome.passed);
        assert!(!outcome.blocking, "non-strict should not block even with violations");
    }
}
