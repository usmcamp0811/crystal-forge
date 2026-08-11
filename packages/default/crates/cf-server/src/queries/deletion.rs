//! Shared deletion-lifecycle response construction.

use crate::api::models::{DeletionBlocker, DeletionEligibility};
use uuid::Uuid;

pub fn eligibility(blockers: Vec<DeletionBlocker>) -> DeletionEligibility {
    DeletionEligibility {
        eligible: blockers.iter().all(|blocker| blocker.removable),
        blockers,
    }
}

pub fn blocker(
    code: &str,
    message: &str,
    removable: bool,
    count: Option<i64>,
    version_ids: Vec<Uuid>,
) -> DeletionBlocker {
    DeletionBlocker {
        kind: code.to_string(),
        code: code.to_string(),
        message: message.to_string(),
        removable,
        count,
        version_ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eligibility_preserves_every_audit_safety_blocker() {
        let immutable_version = Uuid::nil();
        let result = eligibility(vec![
            blocker(
                "policy_immutable_history",
                "Immutable history is retained.",
                false,
                None,
                vec![immutable_version],
            ),
            blocker(
                "policy_assigned",
                "Assignments are retained.",
                false,
                Some(2),
                Vec::new(),
            ),
        ]);

        assert!(!result.eligible);
        assert_eq!(result.blockers.len(), 2);
        assert_eq!(result.blockers[0].version_ids, vec![immutable_version]);
        assert_eq!(result.blockers[1].count, Some(2));
    }

    #[test]
    fn empty_blockers_allow_deletion() {
        assert!(eligibility(Vec::new()).eligible);
    }

    #[test]
    fn removable_draft_dependents_are_eligible_for_transactional_cleanup() {
        let result = eligibility(vec![blocker(
            "mutable_draft_membership",
            "Draft membership will be removed.",
            true,
            Some(1),
            Vec::new(),
        )]);

        assert!(result.eligible);
        assert!(result.blockers[0].removable);
    }

    #[test]
    fn eligibility_serializes_removable_and_retained_classifications() {
        let result = eligibility(vec![
            blocker(
                "mutable_direct_assignment",
                "Direct assignment will be removed.",
                true,
                Some(1),
                Vec::new(),
            ),
            blocker(
                "immutable_source_mapping",
                "Source mapping is retained.",
                false,
                Some(1),
                Vec::new(),
            ),
        ]);

        let value = serde_json::to_value(result).unwrap();
        assert!(!value["eligible"].as_bool().unwrap());
        assert_eq!(value["blockers"][0]["kind"], "mutable_direct_assignment");
        assert!(value["blockers"][0]["removable"].as_bool().unwrap());
        assert!(!value["blockers"][1]["removable"].as_bool().unwrap());
    }
}
