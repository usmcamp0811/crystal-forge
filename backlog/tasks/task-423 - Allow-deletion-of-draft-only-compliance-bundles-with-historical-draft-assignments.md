---
id: TASK-423
title: >-
  Allow deletion of draft-only compliance bundles with historical draft
  assignments
status: In Progress
assignee:
  - '@Matt Camp'
created_date: '2026-08-16 15:13'
updated_date: '2026-08-16 15:48'
labels: []
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/316'
  - packages/default/crates/cf-server/src/queries/compliance.rs
  - packages/default/crates/cf-server/src/handlers/api/compliance.rs
  - >-
    packages/default/crates/cf-server/migrations/0204_compliance_assignment_versions.sql
priority: high
type: bug
ordinal: 418000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Compliance bundle deletion currently blocks permanently whenever any compliance_bundle_assignment_versions row exists for the bundle. This conflates disposable draft-only assignment history with historically effective assignments. A bundle with no accepted or deprecated bundle version and no assignment history attached to an accepted or deprecated bundle version should remain permanently deletable. Draft memberships and draft-only assignment lineage/version rows must be deleted safely with the bundle despite immutable assignment-version safeguards. Bundles with accepted or deprecated versions or assignment history attached to those versions must remain protected and should be archived or deprecated instead. Update deletion preflight and transactional cleanup with explicit tests for draft-only inactive assignment history and historically effective assignment history. Update the UI blocker text to distinguish historical assignment from active assignments when permanent deletion is prohibited.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Draft-only bundles with only draft assignment history can be permanently deleted
- [ ] #2 Bundles with accepted or deprecated assignment history remain protected
- [ ] #3 Draft-only assignment lineage and version rows are cleaned up transactionally
- [ ] #4 Deletion preflight and final deletion use the same historical-effectiveness rule
- [ ] #5 Deletion UI distinguishes active assignments from historical assignment blockers
- [ ] #6 Regression tests cover inactive draft assignment history and accepted assignment history
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Update deletion eligibility to treat assignment history attached to accepted or deprecated bundle versions as the only assignment-history blocker.
2. Add a narrow migration guard allowing deletion of assignment versions only when their referenced bundle versions are mutable draft lineage; preserve immutability for accepted/deprecated history.
3. In the deletion transaction remove draft-only assignment versions in reverse version order, then draft-only assignment lineages and existing disposable memberships/mappings before deleting the bundle.
4. Add database-backed regression coverage for draft-only inactive assignment history and accepted/deprecated assignment history.
5. Update deletion blocker text/API detail so historical assignment blockers are distinguished from active assignments.
6. Run formatting and targeted/server verification in the dedicated worktree.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Preflight completed: TASK-423 and TASK-424 are being implemented together because both correct the mixed legacy/versioned assignment model and both block MR !316. Dedicated worktree is /home/mcamp/code/crystal-forge/TASK-423-424-compliance-assignment-corrections on branch TASK-423-424-compliance-assignment-corrections from dev.

Implemented and pushed commit 11836d03 on origin/TASK-423-424-compliance-assignment-corrections. Draft-only assignment history is now disposable and cleaned in reverse version order; accepted/deprecated assignment history remains protected; migration 0226 narrows immutable triggers for draft cleanup. Server build and offline cargo check passed. Database-backed deletion lifecycle tests remain to be run against the isolated disposable PostgreSQL database.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: OpenAI
created: 2026-08-16 15:17
---
Implementation authorized by user alongside TASK-424; this bug blocks MR !316. Work will be coordinated with TASK-424 in one focused compliance backend change set.
---
<!-- COMMENTS:END -->
