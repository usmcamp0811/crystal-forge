---
id: TASK-423
title: >-
  Allow deletion of draft-only compliance bundles with historical draft
  assignments
status: Backlog
assignee: []
created_date: '2026-08-16 15:13'
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
