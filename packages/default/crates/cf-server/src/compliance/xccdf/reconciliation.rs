//! Deterministic, database-independent CF-native reconciliation planning.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativePolicyIdentity {
    pub lineage_id: Uuid,
    pub version_id: Uuid,
    pub policy_type: String,
    pub semantic_digest: String,
    pub source_rule_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingPolicyIdentity {
    pub lineage_id: Uuid,
    pub version_id: Uuid,
    pub policy_type: String,
    pub semantic_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileDecision {
    ReuseExact {
        local_lineage_id: Uuid,
        local_version_id: Uuid,
    },
    CreateLineageAndVersion {
        portable_lineage_id: Uuid,
        portable_version_id: Uuid,
    },
    CreateVersionInExistingLineage {
        local_lineage_id: Uuid,
        portable_version_id: Uuid,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileConflict {
    VersionDigestMismatch {
        lineage_id: Uuid,
        version_id: Uuid,
        local_digest: String,
        imported_digest: String,
        source_rule_id: String,
    },
    VersionBelongsToDifferentLineage {
        lineage_id: Uuid,
        version_id: Uuid,
        actual_lineage_id: Uuid,
    },
    LineageObjectTypeMismatch {
        lineage_id: Uuid,
    },
    PolicyTypeMismatch {
        lineage_id: Uuid,
        version_id: Uuid,
    },
    InvalidPortableIdentity {
        source_rule_id: String,
    },
}

#[derive(Debug, Clone)]
pub struct NativeReconcileFailure {
    pub conflicts: Vec<ReconcileConflict>,
}

impl fmt::Display for NativeReconcileFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CF-native reconciliation has {} conflict(s)",
            self.conflicts.len()
        )
    }
}

impl std::error::Error for NativeReconcileFailure {}

impl ReconcileConflict {
    pub fn code(&self) -> &'static str {
        match self {
            Self::VersionDigestMismatch { .. } => "CF_NATIVE_VERSION_DIGEST_CONFLICT",
            Self::VersionBelongsToDifferentLineage { .. } => "CF_NATIVE_IDENTITY_CONFLICT",
            Self::LineageObjectTypeMismatch { .. } => "CF_NATIVE_LINEAGE_TYPE_CONFLICT",
            Self::PolicyTypeMismatch { .. } => "CF_NATIVE_POLICY_TYPE_CONFLICT",
            Self::InvalidPortableIdentity { .. } => "CF_NATIVE_INVALID_PORTABLE_IDENTITY",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyReconciliationPlan {
    pub decisions: Vec<(NativePolicyIdentity, ReconcileDecision)>,
    pub conflicts: Vec<ReconcileConflict>,
}

pub fn plan_policy_reconciliation(
    imported: &[NativePolicyIdentity],
    existing: &[ExistingPolicyIdentity],
) -> PolicyReconciliationPlan {
    let mut by_version = HashMap::new();
    let mut by_lineage = HashMap::new();
    for item in existing {
        by_version.insert(item.version_id, item);
        by_lineage.entry(item.lineage_id).or_insert(item);
    }

    let mut imported = imported.to_vec();
    imported.sort_by(|a, b| {
        a.lineage_id
            .cmp(&b.lineage_id)
            .then(a.version_id.cmp(&b.version_id))
            .then(a.source_rule_id.cmp(&b.source_rule_id))
    });

    let mut decisions = Vec::new();
    let mut conflicts = Vec::new();
    for item in imported {
        if item.lineage_id.is_nil() || item.version_id.is_nil() {
            conflicts.push(ReconcileConflict::InvalidPortableIdentity {
                source_rule_id: item.source_rule_id,
            });
            continue;
        }
        if let Some(local) = by_version.get(&item.version_id) {
            if local.lineage_id != item.lineage_id {
                conflicts.push(ReconcileConflict::VersionBelongsToDifferentLineage {
                    lineage_id: item.lineage_id,
                    version_id: item.version_id,
                    actual_lineage_id: local.lineage_id,
                });
            } else if local.policy_type != item.policy_type {
                conflicts.push(ReconcileConflict::PolicyTypeMismatch {
                    lineage_id: item.lineage_id,
                    version_id: item.version_id,
                });
            } else if local.semantic_digest != item.semantic_digest {
                conflicts.push(ReconcileConflict::VersionDigestMismatch {
                    lineage_id: item.lineage_id,
                    version_id: item.version_id,
                    local_digest: local.semantic_digest.clone(),
                    imported_digest: item.semantic_digest.clone(),
                    source_rule_id: item.source_rule_id,
                });
            } else {
                decisions.push((
                    item,
                    ReconcileDecision::ReuseExact {
                        local_lineage_id: local.lineage_id,
                        local_version_id: local.version_id,
                    },
                ));
            }
        } else if by_lineage.contains_key(&item.lineage_id) {
            decisions.push((
                item.clone(),
                ReconcileDecision::CreateVersionInExistingLineage {
                    local_lineage_id: item.lineage_id,
                    portable_version_id: item.version_id,
                },
            ));
        } else {
            decisions.push((
                item.clone(),
                ReconcileDecision::CreateLineageAndVersion {
                    portable_lineage_id: item.lineage_id,
                    portable_version_id: item.version_id,
                },
            ));
        }
    }
    conflicts.sort_by(conflict_order);
    PolicyReconciliationPlan {
        decisions,
        conflicts,
    }
}

fn conflict_order(a: &ReconcileConflict, b: &ReconcileConflict) -> Ordering {
    conflict_key(a).cmp(&conflict_key(b))
}

fn conflict_key(value: &ReconcileConflict) -> (u8, Uuid, Uuid, &'static str) {
    match value {
        ReconcileConflict::VersionDigestMismatch {
            lineage_id,
            version_id,
            ..
        }
        | ReconcileConflict::VersionBelongsToDifferentLineage {
            lineage_id,
            version_id,
            ..
        }
        | ReconcileConflict::PolicyTypeMismatch {
            lineage_id,
            version_id,
        } => (0, *lineage_id, *version_id, value.code()),
        ReconcileConflict::LineageObjectTypeMismatch { lineage_id } => {
            (0, *lineage_id, Uuid::nil(), value.code())
        }
        ReconcileConflict::InvalidPortableIdentity { .. } => {
            (0, Uuid::nil(), Uuid::nil(), value.code())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(lineage_id: u128, version_id: u128, digest: &str) -> NativePolicyIdentity {
        NativePolicyIdentity {
            lineage_id: Uuid::from_u128(lineage_id),
            version_id: Uuid::from_u128(version_id),
            policy_type: "custom_check".into(),
            semantic_digest: digest.into(),
            source_rule_id: format!("rule-{version_id}"),
        }
    }

    fn local(item: &NativePolicyIdentity) -> ExistingPolicyIdentity {
        ExistingPolicyIdentity {
            lineage_id: item.lineage_id,
            version_id: item.version_id,
            policy_type: item.policy_type.clone(),
            semantic_digest: item.semantic_digest.clone(),
        }
    }

    #[test]
    fn exact_version_is_reused() {
        let imported = item(1, 2, "a");
        let plan = plan_policy_reconciliation(&[imported.clone()], &[local(&imported)]);
        assert!(matches!(
            plan.decisions[0].1,
            ReconcileDecision::ReuseExact { .. }
        ));
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn digest_mismatch_is_conflict() {
        let imported = item(1, 2, "new");
        let mut existing = local(&imported);
        existing.semantic_digest = "old".into();
        let plan = plan_policy_reconciliation(&[imported], &[existing]);
        assert_eq!(
            plan.conflicts[0].code(),
            "CF_NATIVE_VERSION_DIGEST_CONFLICT"
        );
    }

    #[test]
    fn missing_version_under_existing_lineage_creates_version() {
        let imported = item(1, 2, "a");
        let existing = ExistingPolicyIdentity {
            lineage_id: imported.lineage_id,
            version_id: Uuid::from_u128(3),
            policy_type: imported.policy_type.clone(),
            semantic_digest: "other".into(),
        };
        let plan = plan_policy_reconciliation(&[imported], &[existing]);
        assert!(matches!(
            plan.decisions[0].1,
            ReconcileDecision::CreateVersionInExistingLineage { .. }
        ));
    }

    #[test]
    fn version_under_wrong_lineage_is_conflict() {
        let imported = item(1, 2, "a");
        let existing = ExistingPolicyIdentity {
            lineage_id: Uuid::from_u128(9),
            version_id: imported.version_id,
            policy_type: imported.policy_type.clone(),
            semantic_digest: imported.semantic_digest.clone(),
        };
        let plan = plan_policy_reconciliation(&[imported], &[existing]);
        assert_eq!(plan.conflicts[0].code(), "CF_NATIVE_IDENTITY_CONFLICT");
    }

    #[test]
    fn mixed_reuse_and_creation_is_planned_without_conflict() {
        let reused = item(1, 2, "a");
        let created = item(3, 4, "b");
        let plan =
            plan_policy_reconciliation(&[reused.clone(), created.clone()], &[local(&reused)]);
        assert!(plan.conflicts.is_empty());
        assert!(matches!(
            plan.decisions[0].1,
            ReconcileDecision::ReuseExact { .. }
        ));
        assert!(matches!(
            plan.decisions[1].1,
            ReconcileDecision::CreateLineageAndVersion { .. }
        ));
    }

    #[test]
    fn conflicts_are_sorted_by_identity() {
        let a = item(2, 2, "new");
        let b = item(1, 2, "new");
        let mut la = local(&a);
        la.semantic_digest = "old".into();
        let mut lb = local(&b);
        lb.semantic_digest = "old".into();
        let plan = plan_policy_reconciliation(&[a, b], &[la, lb]);
        assert_eq!(
            plan.conflicts[0].code(),
            "CF_NATIVE_VERSION_DIGEST_CONFLICT"
        );
        if let ReconcileConflict::VersionDigestMismatch { lineage_id, .. } = plan.conflicts[0] {
            assert_eq!(lineage_id, Uuid::from_u128(1));
        }
    }
}
