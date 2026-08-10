//! Shared deletion-lifecycle response construction.

use crate::api::models::{DeletionBlocker, DeletionEligibility};
use uuid::Uuid;

pub fn eligibility(blockers: Vec<DeletionBlocker>) -> DeletionEligibility {
    DeletionEligibility {
        eligible: blockers.is_empty(),
        blockers,
    }
}

pub fn blocker(
    code: &str,
    message: &str,
    count: Option<i64>,
    version_ids: Vec<Uuid>,
) -> DeletionBlocker {
    DeletionBlocker {
        code: code.to_string(),
        message: message.to_string(),
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
                None,
                vec![immutable_version],
            ),
            blocker(
                "policy_assigned",
                "Assignments are retained.",
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
}
