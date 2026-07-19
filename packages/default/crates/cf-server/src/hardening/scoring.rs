//! Scoring algorithm for systemd service hardening.
//!
//! This module defines the hardening directives we track and how they
//! contribute to a service's overall hardening score.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::types::{DirectiveDetail, RiskLevel};

/// Category of hardening directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectiveCategory {
    /// Namespace isolation (PrivateTmp, ProtectHome, etc.)
    NamespaceIsolation,
    /// Capability restrictions (NoNewPrivileges, CapabilityBoundingSet)
    CapabilityRestriction,
    /// Syscall filtering (SystemCallFilter, SystemCallArchitectures)
    SyscallFiltering,
    /// Resource and access controls (MemoryDenyWriteExecute, etc.)
    ResourceControl,
}

impl DirectiveCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            DirectiveCategory::NamespaceIsolation => "Namespace Isolation",
            DirectiveCategory::CapabilityRestriction => "Capability Restriction",
            DirectiveCategory::SyscallFiltering => "Syscall Filtering",
            DirectiveCategory::ResourceControl => "Resource Control",
        }
    }
}

/// Definition of a hardening directive we track.
#[derive(Debug, Clone)]
pub struct HardeningDirective {
    /// Name of the directive (matches systemd/NixOS config key)
    pub name: &'static str,
    /// Category for grouping
    pub category: DirectiveCategory,
    /// Maximum points this directive can contribute
    pub max_points: i32,
    /// Human-readable description
    pub description: &'static str,
    /// How to evaluate the directive value
    pub evaluator: DirectiveEvaluator,
}

/// How to evaluate a directive's value to determine points.
#[derive(Debug, Clone, Copy)]
pub enum DirectiveEvaluator {
    /// Boolean directive - true = max_points, false/missing = 0
    Boolean,
    /// String directive with specific values worth different points
    /// (e.g., ProtectSystem: "strict" > "full" > "true")
    ProtectSystem,
    /// String directive: "yes"/"read-only"/"tmpfs" patterns
    ProtectHome,
    /// List directive - non-empty list = max_points
    NonEmptyList,
    /// Capability bounding set - empty or restricted = max_points
    CapabilityBoundingSet,
}

/// All hardening directives we track.
///
/// Weights are distributed as follows:
/// - Namespace Isolation: ~30 points (30%)
/// - Capability Restrictions: ~25 points (25%)
/// - Syscall Filtering: ~20 points (20%)
/// - Resource Controls: ~25 points (25%)
///
/// Total: 100 points
pub static HARDENING_DIRECTIVES: &[HardeningDirective] = &[
    // ── Namespace Isolation (30 points) ──────────────────────────────────
    HardeningDirective {
        name: "PrivateTmp",
        category: DirectiveCategory::NamespaceIsolation,
        max_points: 5,
        description: "Mounts private /tmp and /var/tmp directories",
        evaluator: DirectiveEvaluator::Boolean,
    },
    HardeningDirective {
        name: "PrivateDevices",
        category: DirectiveCategory::NamespaceIsolation,
        max_points: 4,
        description: "Restricts access to physical devices",
        evaluator: DirectiveEvaluator::Boolean,
    },
    HardeningDirective {
        name: "PrivateNetwork",
        category: DirectiveCategory::NamespaceIsolation,
        max_points: 3,
        description: "Isolates network namespace (no network access)",
        evaluator: DirectiveEvaluator::Boolean,
    },
    HardeningDirective {
        name: "PrivateUsers",
        category: DirectiveCategory::NamespaceIsolation,
        max_points: 3,
        description: "Runs in isolated user namespace",
        evaluator: DirectiveEvaluator::Boolean,
    },
    HardeningDirective {
        name: "ProtectHome",
        category: DirectiveCategory::NamespaceIsolation,
        max_points: 5,
        description: "Restricts access to home directories",
        evaluator: DirectiveEvaluator::ProtectHome,
    },
    HardeningDirective {
        name: "ProtectSystem",
        category: DirectiveCategory::NamespaceIsolation,
        max_points: 5,
        description: "Mounts /usr, /boot, /efi, /etc read-only",
        evaluator: DirectiveEvaluator::ProtectSystem,
    },
    HardeningDirective {
        name: "ProtectKernelTunables",
        category: DirectiveCategory::NamespaceIsolation,
        max_points: 3,
        description: "Makes kernel tunables read-only",
        evaluator: DirectiveEvaluator::Boolean,
    },
    HardeningDirective {
        name: "ProtectKernelModules",
        category: DirectiveCategory::NamespaceIsolation,
        max_points: 2,
        description: "Prevents loading kernel modules",
        evaluator: DirectiveEvaluator::Boolean,
    },
    // ── Capability Restrictions (25 points) ──────────────────────────────
    HardeningDirective {
        name: "NoNewPrivileges",
        category: DirectiveCategory::CapabilityRestriction,
        max_points: 8,
        description: "Prevents gaining new privileges via setuid/setgid",
        evaluator: DirectiveEvaluator::Boolean,
    },
    HardeningDirective {
        name: "CapabilityBoundingSet",
        category: DirectiveCategory::CapabilityRestriction,
        max_points: 10,
        description: "Limits available Linux capabilities",
        evaluator: DirectiveEvaluator::CapabilityBoundingSet,
    },
    HardeningDirective {
        name: "AmbientCapabilities",
        category: DirectiveCategory::CapabilityRestriction,
        max_points: 7,
        description: "Controls ambient capability set (should be empty)",
        evaluator: DirectiveEvaluator::CapabilityBoundingSet,
    },
    // ── Syscall Filtering (20 points) ────────────────────────────────────
    HardeningDirective {
        name: "SystemCallFilter",
        category: DirectiveCategory::SyscallFiltering,
        max_points: 12,
        description: "Restricts available system calls",
        evaluator: DirectiveEvaluator::NonEmptyList,
    },
    HardeningDirective {
        name: "SystemCallArchitectures",
        category: DirectiveCategory::SyscallFiltering,
        max_points: 8,
        description: "Restricts syscall architectures (e.g., native only)",
        evaluator: DirectiveEvaluator::NonEmptyList,
    },
    // ── Resource Controls (25 points) ────────────────────────────────────
    HardeningDirective {
        name: "MemoryDenyWriteExecute",
        category: DirectiveCategory::ResourceControl,
        max_points: 6,
        description: "Prevents creating writable+executable memory",
        evaluator: DirectiveEvaluator::Boolean,
    },
    HardeningDirective {
        name: "LockPersonality",
        category: DirectiveCategory::ResourceControl,
        max_points: 3,
        description: "Locks execution domain personality",
        evaluator: DirectiveEvaluator::Boolean,
    },
    HardeningDirective {
        name: "RestrictRealtime",
        category: DirectiveCategory::ResourceControl,
        max_points: 3,
        description: "Prevents real-time scheduling",
        evaluator: DirectiveEvaluator::Boolean,
    },
    HardeningDirective {
        name: "RestrictSUIDSGID",
        category: DirectiveCategory::ResourceControl,
        max_points: 4,
        description: "Prevents creating setuid/setgid files",
        evaluator: DirectiveEvaluator::Boolean,
    },
    HardeningDirective {
        name: "RestrictNamespaces",
        category: DirectiveCategory::ResourceControl,
        max_points: 5,
        description: "Restricts namespace creation",
        evaluator: DirectiveEvaluator::Boolean,
    },
    HardeningDirective {
        name: "RestrictAddressFamilies",
        category: DirectiveCategory::ResourceControl,
        max_points: 4,
        description: "Restricts available address families",
        evaluator: DirectiveEvaluator::NonEmptyList,
    },
];

/// Evaluate a directive's value and return points earned.
fn evaluate_directive(directive: &HardeningDirective, value: &Value) -> (bool, i32) {
    match directive.evaluator {
        DirectiveEvaluator::Boolean => {
            let enabled = value.as_bool().unwrap_or(false);
            (enabled, if enabled { directive.max_points } else { 0 })
        }
        DirectiveEvaluator::ProtectSystem => {
            // ProtectSystem can be: false, true, "full", "strict"
            // strict > full > true > false
            match value {
                Value::Bool(true) => (true, directive.max_points / 2),
                Value::String(s) => match s.as_str() {
                    "strict" => (true, directive.max_points),
                    "full" => (true, (directive.max_points * 3) / 4),
                    "true" => (true, directive.max_points / 2),
                    _ => (false, 0),
                },
                _ => (false, 0),
            }
        }
        DirectiveEvaluator::ProtectHome => {
            // ProtectHome can be: false, true, "read-only", "tmpfs"
            match value {
                Value::Bool(true) => (true, directive.max_points / 2),
                Value::String(s) => match s.as_str() {
                    "tmpfs" => (true, directive.max_points),
                    "read-only" => (true, (directive.max_points * 3) / 4),
                    "yes" | "true" => (true, directive.max_points / 2),
                    _ => (false, 0),
                },
                _ => (false, 0),
            }
        }
        DirectiveEvaluator::NonEmptyList => {
            // For list-based directives, any non-empty configuration is good
            let enabled = match value {
                Value::Array(arr) => !arr.is_empty(),
                Value::String(s) => !s.is_empty() && s != "~",
                _ => false,
            };
            (enabled, if enabled { directive.max_points } else { 0 })
        }
        DirectiveEvaluator::CapabilityBoundingSet => {
            // For capabilities, empty or restricted is better
            // Full capabilities (or unset) = 0 points
            // Restricted set = partial points
            // Empty set = max points
            match value {
                Value::Array(arr) if arr.is_empty() => (true, directive.max_points),
                Value::String(s) if s.is_empty() || s == "~" => (true, directive.max_points),
                Value::Array(arr) => {
                    // Partial credit for restricted sets
                    let restricted = arr.len() < 20; // Arbitrary threshold
                    (
                        restricted,
                        if restricted {
                            directive.max_points / 2
                        } else {
                            0
                        },
                    )
                }
                Value::Null => (false, 0), // Not configured
                _ => (false, 0),
            }
        }
    }
}

/// Result of calculating a service's hardening score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceScoreResult {
    /// Overall score (0-100)
    pub score: i32,
    /// Risk level derived from score
    pub risk_level: RiskLevel,
    /// Details for each directive
    pub directives: Vec<DirectiveDetail>,
    /// Count of enabled directives
    pub enabled_count: i32,
    /// Count of disabled directives
    pub disabled_count: i32,
    /// Count of missing/unconfigured directives
    pub missing_count: i32,
}

/// Calculate the hardening score for a service.
///
/// Takes the `serviceConfig` object from a NixOS systemd service definition
/// and evaluates all tracked hardening directives.
pub fn calculate_service_score(service_config: &Value) -> ServiceScoreResult {
    let mut total_points = 0;
    let mut max_total_points = 0;
    let mut directives = Vec::new();
    let mut enabled_count = 0;
    let mut disabled_count = 0;
    let mut missing_count = 0;

    for directive in HARDENING_DIRECTIVES {
        let value = service_config.get(directive.name);
        let (enabled, points) = match value {
            Some(v) => evaluate_directive(directive, v),
            None => (false, 0),
        };

        total_points += points;
        max_total_points += directive.max_points;

        if value.is_some() {
            if enabled {
                enabled_count += 1;
            } else {
                disabled_count += 1;
            }
        } else {
            missing_count += 1;
        }

        directives.push(DirectiveDetail {
            name: directive.name.to_string(),
            enabled,
            value: value.cloned().unwrap_or(Value::Null),
            points,
            max_points: directive.max_points,
            category: directive.category.as_str().to_string(),
            description: directive.description.to_string(),
        });
    }

    // Calculate percentage score (0-100)
    let score = if max_total_points > 0 {
        (total_points * 100) / max_total_points
    } else {
        0
    };

    ServiceScoreResult {
        score,
        risk_level: RiskLevel::from_score(score),
        directives,
        enabled_count,
        disabled_count,
        missing_count,
    }
}

/// Calculate aggregate statistics from multiple service scores.
pub fn calculate_scan_statistics(
    service_scores: &[ServiceScoreResult],
) -> (i32, i32, i32, i32, i32, Option<i32>) {
    let mut well_hardened = 0;
    let mut moderately_hardened = 0;
    let mut poorly_hardened = 0;
    let mut vulnerable = 0;
    let mut total_score = 0;

    for result in service_scores {
        total_score += result.score;
        match result.risk_level {
            RiskLevel::WellHardened => well_hardened += 1,
            RiskLevel::ModeratelyHardened => moderately_hardened += 1,
            RiskLevel::PoorlyHardened => poorly_hardened += 1,
            RiskLevel::Vulnerable => vulnerable += 1,
        }
    }

    let overall_score = if !service_scores.is_empty() {
        Some(total_score / service_scores.len() as i32)
    } else {
        None
    };

    (
        well_hardened,
        moderately_hardened,
        poorly_hardened,
        vulnerable,
        service_scores.len() as i32,
        overall_score,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_well_hardened_service() {
        let config = json!({
            "PrivateTmp": true,
            "PrivateDevices": true,
            "PrivateNetwork": true,
            "PrivateUsers": true,
            "ProtectHome": "tmpfs",
            "ProtectSystem": "strict",
            "ProtectKernelTunables": true,
            "ProtectKernelModules": true,
            "NoNewPrivileges": true,
            "CapabilityBoundingSet": [],
            "AmbientCapabilities": [],
            "SystemCallFilter": ["@system-service"],
            "SystemCallArchitectures": ["native"],
            "MemoryDenyWriteExecute": true,
            "LockPersonality": true,
            "RestrictRealtime": true,
            "RestrictSUIDSGID": true,
            "RestrictNamespaces": true,
            "RestrictAddressFamilies": ["AF_UNIX", "AF_INET"]
        });

        let result = calculate_service_score(&config);
        assert_eq!(result.risk_level, RiskLevel::WellHardened);
        assert!(result.score >= 80);
        assert_eq!(result.enabled_count, HARDENING_DIRECTIVES.len() as i32);
        assert_eq!(result.disabled_count, 0);
        assert_eq!(result.missing_count, 0);
    }

    #[test]
    fn test_vulnerable_service() {
        // Empty config - no hardening
        let config = json!({});

        let result = calculate_service_score(&config);
        assert_eq!(result.risk_level, RiskLevel::Vulnerable);
        assert_eq!(result.score, 0);
        assert_eq!(result.enabled_count, 0);
        assert_eq!(result.disabled_count, 0);
        assert_eq!(result.missing_count, HARDENING_DIRECTIVES.len() as i32);
    }

    #[test]
    fn test_partial_hardening() {
        let config = json!({
            "PrivateTmp": true,
            "NoNewPrivileges": true,
            "ProtectSystem": "full"
        });

        let result = calculate_service_score(&config);
        assert!(result.score > 0);
        assert!(result.score < 50); // Should be poorly hardened or vulnerable
        assert_eq!(result.enabled_count, 3);
    }

    #[test]
    fn test_protect_system_levels() {
        // strict = max points
        let strict = json!({"ProtectSystem": "strict"});
        let strict_result = calculate_service_score(&strict);
        let strict_detail = strict_result
            .directives
            .iter()
            .find(|d| d.name == "ProtectSystem")
            .unwrap();
        assert_eq!(strict_detail.points, strict_detail.max_points);

        // full = 3/4 points
        let full = json!({"ProtectSystem": "full"});
        let full_result = calculate_service_score(&full);
        let full_detail = full_result
            .directives
            .iter()
            .find(|d| d.name == "ProtectSystem")
            .unwrap();
        assert!(full_detail.points < full_detail.max_points);
        assert!(full_detail.points > 0);

        // true = 1/2 points
        let bool_true = json!({"ProtectSystem": true});
        let bool_result = calculate_service_score(&bool_true);
        let bool_detail = bool_result
            .directives
            .iter()
            .find(|d| d.name == "ProtectSystem")
            .unwrap();
        assert!(bool_detail.points < full_detail.points);
    }

    #[test]
    fn test_risk_level_thresholds() {
        assert_eq!(RiskLevel::from_score(100), RiskLevel::WellHardened);
        assert_eq!(RiskLevel::from_score(80), RiskLevel::WellHardened);
        assert_eq!(RiskLevel::from_score(79), RiskLevel::ModeratelyHardened);
        assert_eq!(RiskLevel::from_score(60), RiskLevel::ModeratelyHardened);
        assert_eq!(RiskLevel::from_score(59), RiskLevel::PoorlyHardened);
        assert_eq!(RiskLevel::from_score(40), RiskLevel::PoorlyHardened);
        assert_eq!(RiskLevel::from_score(39), RiskLevel::Vulnerable);
        assert_eq!(RiskLevel::from_score(0), RiskLevel::Vulnerable);
    }
}
