//! Checks that unknown coverage does not replace the design's known-issue order.

use super::policy_matrix::{Summary, worst_first_key};

#[test]
fn known_issues_keep_design_weights_and_precede_unavailable_coverage() {
    let failure = Summary {
        failed: 1,
        applicable: 1,
        ..Summary::default()
    };
    let warnings = Summary {
        warnings: 8,
        applicable: 8,
        ..Summary::default()
    };
    let missing = Summary {
        unknown: 128,
        applicable: 128,
        ..Summary::default()
    };
    assert!(worst_first_key(&failure, true) > worst_first_key(&warnings, true));
    assert!(worst_first_key(&warnings, true) > worst_first_key(&missing, false));
    assert!(worst_first_key(&missing, false) > worst_first_key(&Summary::default(), true));
}
