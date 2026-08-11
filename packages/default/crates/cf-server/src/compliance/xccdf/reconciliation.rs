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

// ── Bundle reconciliation ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBundleIdentity {
    pub lineage_id: Uuid,
    pub version_id: Uuid,
    pub semantic_digest: String,
    /// Ordered bundle membership: (policy_version_id, selected) sorted by policy_order.
    pub members: Vec<(Uuid, bool)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingBundleIdentity {
    pub lineage_id: Uuid,
    pub version_id: Uuid,
    pub semantic_digest: String,
    /// Ordered bundle membership: (policy_version_id, selected) sorted by policy_order.
    pub members: Vec<(Uuid, bool)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleReconcileDecision {
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
pub enum BundleReconcileConflict {
    VersionDigestMismatch {
        lineage_id: Uuid,
        version_id: Uuid,
        local_digest: String,
        imported_digest: String,
    },
    VersionBelongsToDifferentLineage {
        lineage_id: Uuid,
        version_id: Uuid,
        actual_lineage_id: Uuid,
    },
    BundleMembershipMismatch {
        lineage_id: Uuid,
        version_id: Uuid,
    },
}

impl BundleReconcileConflict {
    pub fn code(&self) -> &'static str {
        match self {
            Self::VersionDigestMismatch { .. } => "CF_NATIVE_BUNDLE_DIGEST_CONFLICT",
            Self::VersionBelongsToDifferentLineage { .. } => "CF_NATIVE_BUNDLE_IDENTITY_CONFLICT",
            Self::BundleMembershipMismatch { .. } => "CF_NATIVE_BUNDLE_MEMBERSHIP_CONFLICT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleReconciliationPlan {
    pub decision: Option<BundleReconcileDecision>,
    pub conflicts: Vec<BundleReconcileConflict>,
}

pub fn plan_bundle_reconciliation(
    imported: &NativeBundleIdentity,
    existing: Option<&ExistingBundleIdentity>,
) -> BundleReconciliationPlan {
    if imported.lineage_id.is_nil() || imported.version_id.is_nil() {
        return BundleReconciliationPlan {
            decision: None,
            conflicts: vec![],
        };
    }

    match existing {
        Some(local) => {
            // Exact match: same lineage, version, digest, and membership
            if local.lineage_id == imported.lineage_id
                && local.version_id == imported.version_id
                && local.semantic_digest == imported.semantic_digest
                && local.members == imported.members
            {
                BundleReconciliationPlan {
                    decision: Some(BundleReconcileDecision::ReuseExact {
                        local_lineage_id: local.lineage_id,
                        local_version_id: local.version_id,
                    }),
                    conflicts: vec![],
                }
            } else if local.version_id == imported.version_id {
                // Same version ID but different lineage, digest, or membership = conflict
                let mut conflicts = vec![];
                if local.lineage_id != imported.lineage_id {
                    conflicts.push(BundleReconcileConflict::VersionBelongsToDifferentLineage {
                        lineage_id: imported.lineage_id,
                        version_id: imported.version_id,
                        actual_lineage_id: local.lineage_id,
                    });
                } else if local.semantic_digest != imported.semantic_digest {
                    conflicts.push(BundleReconcileConflict::VersionDigestMismatch {
                        lineage_id: imported.lineage_id,
                        version_id: imported.version_id,
                        local_digest: local.semantic_digest.clone(),
                        imported_digest: imported.semantic_digest.clone(),
                    });
                } else if local.members != imported.members {
                    conflicts.push(BundleReconcileConflict::BundleMembershipMismatch {
                        lineage_id: imported.lineage_id,
                        version_id: imported.version_id,
                    });
                }
                BundleReconciliationPlan {
                    decision: None,
                    conflicts,
                }
            } else if local.lineage_id == imported.lineage_id {
                // Same lineage, new version
                BundleReconciliationPlan {
                    decision: Some(BundleReconcileDecision::CreateVersionInExistingLineage {
                        local_lineage_id: local.lineage_id,
                        portable_version_id: imported.version_id,
                    }),
                    conflicts: vec![],
                }
            } else {
                // Different lineage, different version = conflict
                BundleReconciliationPlan {
                    decision: None,
                    conflicts: vec![BundleReconcileConflict::VersionBelongsToDifferentLineage {
                        lineage_id: imported.lineage_id,
                        version_id: imported.version_id,
                        actual_lineage_id: local.lineage_id,
                    }],
                }
            }
        }
        None => {
            // New lineage and version
            BundleReconciliationPlan {
                decision: Some(BundleReconcileDecision::CreateLineageAndVersion {
                    portable_lineage_id: imported.lineage_id,
                    portable_version_id: imported.version_id,
                }),
                conflicts: vec![],
            }
        }
    }
}

#[cfg(test)]
mod bundle_tests {
    use super::*;

    fn bundle(
        lineage: u128,
        version: u128,
        digest: &str,
        members: Vec<u128>,
    ) -> NativeBundleIdentity {
        NativeBundleIdentity {
            lineage_id: Uuid::from_u128(lineage),
            version_id: Uuid::from_u128(version),
            semantic_digest: digest.into(),
            members: members
                .into_iter()
                .map(|m| (Uuid::from_u128(m), true))
                .collect(),
        }
    }

    fn existing_bundle(
        lineage: u128,
        version: u128,
        digest: &str,
        members: Vec<u128>,
    ) -> ExistingBundleIdentity {
        ExistingBundleIdentity {
            lineage_id: Uuid::from_u128(lineage),
            version_id: Uuid::from_u128(version),
            semantic_digest: digest.into(),
            members: members
                .into_iter()
                .map(|m| (Uuid::from_u128(m), true))
                .collect(),
        }
    }

    /// Build an existing bundle with explicit (member, selected) pairs in order.
    fn existing_bundle_with_selection(
        lineage: u128,
        version: u128,
        digest: &str,
        members: Vec<(u128, bool)>,
    ) -> ExistingBundleIdentity {
        ExistingBundleIdentity {
            lineage_id: Uuid::from_u128(lineage),
            version_id: Uuid::from_u128(version),
            semantic_digest: digest.into(),
            members: members
                .into_iter()
                .map(|(m, selected)| (Uuid::from_u128(m), selected))
                .collect(),
        }
    }

    #[test]
    fn new_lineage_and_version() {
        let imported = bundle(1, 2, "digest-a", vec![10, 20]);
        let plan = plan_bundle_reconciliation(&imported, None);
        assert!(matches!(
            plan.decision,
            Some(BundleReconcileDecision::CreateLineageAndVersion { .. })
        ));
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn existing_lineage_new_version() {
        let imported = bundle(1, 2, "digest-a", vec![10, 20]);
        let existing = existing_bundle(1, 3, "digest-b", vec![10, 20]);
        let plan = plan_bundle_reconciliation(&imported, Some(&existing));
        assert!(matches!(
            plan.decision,
            Some(BundleReconcileDecision::CreateVersionInExistingLineage { .. })
        ));
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn exact_match_version_and_digest_and_membership() {
        let imported = bundle(1, 2, "digest-a", vec![10, 20]);
        let existing = existing_bundle(1, 2, "digest-a", vec![10, 20]);
        let plan = plan_bundle_reconciliation(&imported, Some(&existing));
        assert!(matches!(
            plan.decision,
            Some(BundleReconcileDecision::ReuseExact { .. })
        ));
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn same_version_wrong_lineage_conflict() {
        let imported = bundle(1, 2, "digest-a", vec![10, 20]);
        let existing = existing_bundle(99, 2, "digest-a", vec![10, 20]);
        let plan = plan_bundle_reconciliation(&imported, Some(&existing));
        assert!(plan.decision.is_none());
        assert_eq!(plan.conflicts.len(), 1);
        assert!(matches!(
            plan.conflicts[0],
            BundleReconcileConflict::VersionBelongsToDifferentLineage { .. }
        ));
    }

    #[test]
    fn same_version_different_digest_conflict() {
        let imported = bundle(1, 2, "digest-new", vec![10, 20]);
        let existing = existing_bundle(1, 2, "digest-old", vec![10, 20]);
        let plan = plan_bundle_reconciliation(&imported, Some(&existing));
        assert!(plan.decision.is_none());
        assert_eq!(plan.conflicts.len(), 1);
        assert!(matches!(
            plan.conflicts[0],
            BundleReconcileConflict::VersionDigestMismatch { .. }
        ));
    }

    #[test]
    fn same_version_digest_different_membership_conflict() {
        let imported = bundle(1, 2, "digest-a", vec![10, 20, 30]);
        let existing = existing_bundle(1, 2, "digest-a", vec![10, 20]);
        let plan = plan_bundle_reconciliation(&imported, Some(&existing));
        assert!(plan.decision.is_none());
        assert_eq!(plan.conflicts.len(), 1);
        assert!(matches!(
            plan.conflicts[0],
            BundleReconcileConflict::BundleMembershipMismatch { .. }
        ));
    }

    #[test]
    fn same_version_digest_different_order_conflict() {
        let imported = bundle(1, 2, "digest-a", vec![10, 20]);
        let existing = existing_bundle(1, 2, "digest-a", vec![20, 10]);
        let plan = plan_bundle_reconciliation(&imported, Some(&existing));
        assert!(plan.decision.is_none());
        assert_eq!(plan.conflicts.len(), 1);
        assert!(matches!(
            plan.conflicts[0],
            BundleReconcileConflict::BundleMembershipMismatch { .. }
        ));
    }

    #[test]
    fn nil_version_id_does_not_crash() {
        let imported = bundle(1, 0, "digest-a", vec![10, 20]);
        let plan = plan_bundle_reconciliation(&imported, None);
        assert!(plan.decision.is_none());
    }

    #[test]
    fn nil_lineage_id_does_not_crash() {
        let imported = bundle(0, 2, "digest-a", vec![10, 20]);
        let plan = plan_bundle_reconciliation(&imported, None);
        assert!(plan.decision.is_none());
    }

    #[test]
    fn same_version_digest_membership_different_selected_conflict() {
        // Same lineage, same version ID, same digest, same members, same order,
        // but a different `selected` value: must be a membership conflict.
        let imported = bundle(1, 2, "digest-a", vec![10, 20]);
        let existing =
            existing_bundle_with_selection(1, 2, "digest-a", vec![(10, true), (20, false)]);
        let plan = plan_bundle_reconciliation(&imported, Some(&existing));
        assert!(plan.decision.is_none());
        assert_eq!(plan.conflicts.len(), 1);
        assert!(matches!(
            plan.conflicts[0],
            BundleReconcileConflict::BundleMembershipMismatch { .. }
        ));
    }
}
