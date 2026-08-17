//! The authoritative lineage/version decision rule for effective policy sets.
//!
//! Crystal Forge resolves an effective policy set to **at most one version per
//! policy lineage**. Two versions of the same lineage must never silently
//! coexist: either a higher-specificity source wins, or the situation is a
//! conflict that a human must resolve.
//!
//! This module holds that decision as a pure function so that every consumer
//! applies identical semantics:
//!
//! * `cf-server`'s database-backed resolver (`compliance::resolver`), which
//!   merges bundle baselines, assignment additions, and direct environment or
//!   system policies; and
//! * the offline `cf-nixos-module` generator, which merges policies across
//!   exported artifacts.
//!
//! Import *reconciliation* (`xccdf::reconciliation`) answers a different
//! question — "does this imported version already exist locally?" — and must
//! not be used as a substitute for effective-set resolution.

use uuid::Uuid;

/// How specific the source of a policy selection is.
///
/// A more specific source overrides a less specific one for the same lineage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Specificity {
    /// Selected by a compliance bundle baseline.
    BundleBaseline = 0,
    /// Selected directly on an environment.
    Environment = 1,
    /// Selected directly on a system.
    System = 2,
}

/// The outcome of merging one candidate into an existing lineage entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineageDecision {
    /// The lineage has no entry yet; insert the candidate.
    Insert,
    /// Same exact version. Keep the existing entry, and adopt the candidate's
    /// source metadata when it is strictly more specific.
    Deduplicate { adopt_candidate_metadata: bool },
    /// A strictly more specific source selected a different version of the same
    /// lineage; the candidate replaces the existing entry.
    Replace,
    /// A less specific source selected a different version; the candidate is
    /// suppressed and recorded only as non-authoritative provenance.
    Suppress,
    /// Two sources of equal specificity selected different versions of one
    /// lineage. Crystal Forge never picks a winner here.
    Conflict {
        lineage_id: Uuid,
        existing_version_id: Uuid,
        candidate_version_id: Uuid,
    },
}

/// Apply the authoritative lineage/version rule.
///
/// `existing` is the currently selected `(version_id, specificity)` for
/// `lineage_id`, or `None` when this is the first candidate for the lineage.
pub fn resolve_lineage_candidate(
    lineage_id: Uuid,
    existing: Option<(Uuid, Specificity)>,
    candidate_version_id: Uuid,
    candidate_specificity: Specificity,
) -> LineageDecision {
    let Some((existing_version_id, existing_specificity)) = existing else {
        return LineageDecision::Insert;
    };

    if existing_version_id == candidate_version_id {
        return LineageDecision::Deduplicate {
            adopt_candidate_metadata: candidate_specificity > existing_specificity,
        };
    }

    match candidate_specificity.cmp(&existing_specificity) {
        std::cmp::Ordering::Greater => LineageDecision::Replace,
        std::cmp::Ordering::Less => LineageDecision::Suppress,
        std::cmp::Ordering::Equal => LineageDecision::Conflict {
            lineage_id,
            existing_version_id,
            candidate_version_id,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uuid(byte: u8) -> Uuid {
        Uuid::from_bytes([byte; 16])
    }

    #[test]
    fn first_candidate_for_a_lineage_is_inserted() {
        assert_eq!(
            resolve_lineage_candidate(uuid(1), None, uuid(2), Specificity::BundleBaseline),
            LineageDecision::Insert
        );
    }

    #[test]
    fn same_version_at_same_specificity_deduplicates() {
        assert_eq!(
            resolve_lineage_candidate(
                uuid(1),
                Some((uuid(2), Specificity::BundleBaseline)),
                uuid(2),
                Specificity::BundleBaseline,
            ),
            LineageDecision::Deduplicate {
                adopt_candidate_metadata: false
            }
        );
    }

    #[test]
    fn same_version_at_higher_specificity_adopts_candidate_metadata() {
        assert_eq!(
            resolve_lineage_candidate(
                uuid(1),
                Some((uuid(2), Specificity::BundleBaseline)),
                uuid(2),
                Specificity::System,
            ),
            LineageDecision::Deduplicate {
                adopt_candidate_metadata: true
            }
        );
    }

    #[test]
    fn different_version_at_higher_specificity_replaces() {
        assert_eq!(
            resolve_lineage_candidate(
                uuid(1),
                Some((uuid(2), Specificity::BundleBaseline)),
                uuid(3),
                Specificity::Environment,
            ),
            LineageDecision::Replace
        );
    }

    #[test]
    fn different_version_at_lower_specificity_is_suppressed() {
        assert_eq!(
            resolve_lineage_candidate(
                uuid(1),
                Some((uuid(2), Specificity::System)),
                uuid(3),
                Specificity::BundleBaseline,
            ),
            LineageDecision::Suppress
        );
    }

    #[test]
    fn different_versions_at_equal_specificity_conflict() {
        assert_eq!(
            resolve_lineage_candidate(
                uuid(1),
                Some((uuid(2), Specificity::BundleBaseline)),
                uuid(3),
                Specificity::BundleBaseline,
            ),
            LineageDecision::Conflict {
                lineage_id: uuid(1),
                existing_version_id: uuid(2),
                candidate_version_id: uuid(3),
            }
        );
    }

    #[test]
    fn specificity_ordering_matches_crystal_forge_precedence() {
        assert!(Specificity::BundleBaseline < Specificity::Environment);
        assert!(Specificity::Environment < Specificity::System);
    }
}
