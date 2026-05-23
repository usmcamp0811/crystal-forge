// CVE threshold policy evaluation service
// Enhanced CVE gating with per-severity thresholds and actions

use sqlx::PgPool;
use std::collections::HashMap;

use crate::models::deployment_policies::{CveThresholdConfig, SeverityAction};

/// Result of CVE threshold policy evaluation
#[derive(Debug, Clone)]
pub struct CveThresholdResult {
    pub deployment_allowed: bool,
    pub violations: Vec<ThresholdViolation>,
    pub warnings: Vec<String>,
}

/// A threshold violation
#[derive(Debug, Clone)]
pub struct ThresholdViolation {
    pub severity: String,
    pub count: u32,
    pub max_allowed: u32,
    pub action: SeverityAction,
}

/// CVE count by severity from database
#[derive(Debug, Clone)]
struct CveCountBySeverity {
    critical: u32,
    high: u32,
    medium: u32,
    low: u32,
}

/// Evaluate CVE threshold policy against a derivation's CVE scan
pub async fn check_cve_thresholds(
    pool: &PgPool,
    derivation_id: i32,
    config: &CveThresholdConfig,
) -> Result<CveThresholdResult, sqlx::Error> {
    // Fetch latest CVE scan for derivation
    let scan = get_latest_cve_scan(pool, derivation_id).await?;

    match scan {
        None => handle_no_scan(config),
        Some(cve_counts) => evaluate_thresholds(&cve_counts, config),
    }
}

/// Handle the case when no CVE scan exists
fn handle_no_scan(config: &CveThresholdConfig) -> Result<CveThresholdResult, sqlx::Error> {
    match config.no_scan_behavior.as_str() {
        "block" => Ok(CveThresholdResult {
            deployment_allowed: false,
            violations: vec![],
            warnings: vec!["No CVE scan found, blocking deployment".to_string()],
        }),
        "skip" => Ok(CveThresholdResult {
            deployment_allowed: true,
            violations: vec![],
            warnings: vec!["No CVE scan found, skipping CVE checks".to_string()],
        }),
        "warn" => Ok(CveThresholdResult {
            deployment_allowed: true,
            violations: vec![],
            warnings: vec!["No CVE scan found, allowing deployment with warning".to_string()],
        }),
        _ => Ok(CveThresholdResult {
            deployment_allowed: false,
            violations: vec![],
            warnings: vec![format!(
                "Unknown no_scan_behavior: {}, blocking deployment",
                config.no_scan_behavior
            )],
        }),
    }
}

/// Evaluate thresholds against CVE counts
fn evaluate_thresholds(
    cve_counts: &CveCountBySeverity,
    config: &CveThresholdConfig,
) -> Result<CveThresholdResult, sqlx::Error> {
    let mut violations = Vec::new();
    let mut warnings = Vec::new();
    let mut should_block = false;

    // Check each configured threshold
    for (severity, threshold) in &config.thresholds {
        let count = match severity.as_str() {
            "critical" => cve_counts.critical,
            "high" => cve_counts.high,
            "medium" => cve_counts.medium,
            "low" => cve_counts.low,
            _ => {
                warnings.push(format!("Unknown severity level: {}", severity));
                continue;
            }
        };

        if count > threshold.max {
            let violation = ThresholdViolation {
                severity: severity.clone(),
                count,
                max_allowed: threshold.max,
                action: threshold.action.clone(),
            };

            match threshold.action {
                SeverityAction::Block => {
                    should_block = true;
                    violations.push(violation);
                    warnings.push(format!(
                        "BLOCK: {} {} CVEs exceeds threshold of {} (action: block)",
                        count, severity, threshold.max
                    ));
                }
                SeverityAction::Warn => {
                    violations.push(violation);
                    warnings.push(format!(
                        "WARN: {} {} CVEs exceeds threshold of {} (action: warn)",
                        count, severity, threshold.max
                    ));
                }
            }
        }
    }

    Ok(CveThresholdResult {
        deployment_allowed: !should_block,
        violations,
        warnings,
    })
}

/// Fetch latest CVE scan counts for a derivation
async fn get_latest_cve_scan(
    pool: &PgPool,
    derivation_id: i32,
) -> Result<Option<CveCountBySeverity>, sqlx::Error> {
    let result = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        r#"
        SELECT
            COALESCE(SUM(CASE WHEN severity = 'critical' THEN 1 ELSE 0 END), 0) as critical,
            COALESCE(SUM(CASE WHEN severity = 'high' THEN 1 ELSE 0 END), 0) as high,
            COALESCE(SUM(CASE WHEN severity = 'medium' THEN 1 ELSE 0 END), 0) as medium,
            COALESCE(SUM(CASE WHEN severity = 'low' THEN 1 ELSE 0 END), 0) as low
        FROM cve_scan_results csr
        JOIN cve_scans cs ON csr.scan_id = cs.id
        WHERE cs.derivation_id = $1
          AND cs.id = (
              SELECT id FROM cve_scans
              WHERE derivation_id = $1
              ORDER BY created_at DESC
              LIMIT 1
          )
        "#,
    )
    .bind(derivation_id)
    .fetch_optional(pool)
    .await?;

    Ok(result.map(|row| CveCountBySeverity {
        critical: row.0 as u32,
        high: row.1 as u32,
        medium: row.2 as u32,
        low: row.3 as u32,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_thresholds_all_pass() {
        let counts = CveCountBySeverity {
            critical: 0,
            high: 1,
            medium: 5,
            low: 10,
        };

        let mut thresholds = HashMap::new();
        thresholds.insert(
            "critical".to_string(),
            crate::models::deployment_policies::SeverityThreshold {
                max: 0,
                action: SeverityAction::Block,
            },
        );
        thresholds.insert(
            "high".to_string(),
            crate::models::deployment_policies::SeverityThreshold {
                max: 2,
                action: SeverityAction::Block,
            },
        );

        let config = CveThresholdConfig {
            description: "Test config".to_string(),
            thresholds,
            no_scan_behavior: "block".to_string(),
            allow_justifications: false,
            require_acknowledgment: false,
        };

        let result = evaluate_thresholds(&counts, &config).unwrap();
        assert!(result.deployment_allowed);
        assert_eq!(result.violations.len(), 0);
    }

    #[test]
    fn test_evaluate_thresholds_block() {
        let counts = CveCountBySeverity {
            critical: 1,
            high: 0,
            medium: 0,
            low: 0,
        };

        let mut thresholds = HashMap::new();
        thresholds.insert(
            "critical".to_string(),
            crate::models::deployment_policies::SeverityThreshold {
                max: 0,
                action: SeverityAction::Block,
            },
        );

        let config = CveThresholdConfig {
            description: "Test config".to_string(),
            thresholds,
            no_scan_behavior: "block".to_string(),
            allow_justifications: false,
            require_acknowledgment: false,
        };

        let result = evaluate_thresholds(&counts, &config).unwrap();
        assert!(!result.deployment_allowed);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].severity, "critical");
    }

    #[test]
    fn test_evaluate_thresholds_warn_only() {
        let counts = CveCountBySeverity {
            critical: 0,
            high: 0,
            medium: 15,
            low: 0,
        };

        let mut thresholds = HashMap::new();
        thresholds.insert(
            "medium".to_string(),
            crate::models::deployment_policies::SeverityThreshold {
                max: 10,
                action: SeverityAction::Warn,
            },
        );

        let config = CveThresholdConfig {
            description: "Test config".to_string(),
            thresholds,
            no_scan_behavior: "block".to_string(),
            allow_justifications: false,
            require_acknowledgment: false,
        };

        let result = evaluate_thresholds(&counts, &config).unwrap();
        assert!(result.deployment_allowed); // Warn doesn't block
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].action, SeverityAction::Warn);
    }
}
