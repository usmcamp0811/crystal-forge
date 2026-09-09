---
id: TASK-443
title: Complete zero-downtime derivation-path uniqueness transition
status: Backlog
assignee: []
created_date: '2026-08-30 21:54'
updated_date: '2026-08-30 21:56'
labels:
  - backend
  - database
  - dependency-graph
  - zero-downtime
dependencies:
  - TASK-441
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/322'
modified_files:
  - packages/default/crates/cf-server/migrations/
  - packages/default/crates/cf-server/src/queries/derivations.rs
  - packages/default/crates/cf-server/src/models/evaluate_with_policies.rs
  - packages/default/crates/cf-server/tests/dependency_build_plan_lifecycle.rs
  - checks/server-regressions/default.nix
priority: high
type: bug
ordinal: 452000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Follow-up release for TASK-441. After the compatibility release is deployed to all server processes, add a new forward migration that removes the legacy global derivations.derivation_path uniqueness constraint. Enable separate commit-owned derivation rows for the same Nix derivation path, remove the temporary compatibility failure path, and verify evaluation-attempt ownership and dependency-plan barriers remain independent across commits. This must be a separate deployment because an older server executes ON CONFLICT (derivation_path), which PostgreSQL rejects after the matching unique constraint is removed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A new forward migration removes derivations_derivation_path_unique only after the compatibility release is deployed to all server processes
- [ ] #2 Two different commits can persist independent derivation rows with the same derivation_path and distinct evaluation_attempt_id values
- [ ] #3 No production query depends on ON CONFLICT (derivation_path) before the constraint is removed
- [ ] #4 The temporary compatibility failure representation is removed after duplicate commit-owned rows are enabled
- [ ] #5 Migration and PostgreSQL regressions prove upgrade from the compatibility release and independent ready barriers for both commits
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
TASK-443 is the second deployment phase selected by the user. It must begin only after the compatibility changes from MR !322 are deployed to every server process. TASK-441 remains the parent objective but is not a dependency because TASK-441 cannot be complete until this phase finishes.
<!-- SECTION:NOTES:END -->
