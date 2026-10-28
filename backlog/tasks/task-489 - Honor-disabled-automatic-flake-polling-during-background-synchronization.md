---
id: TASK-489
title: Honor disabled automatic flake polling during background synchronization
status: Backlog
assignee: []
created_date: '2026-10-16 04:35'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-server/src/queries/flakes.rs
  - packages/default/crates/cf-server/src/server/mod.rs
  - packages/default/crates/cf-server/src/flake/commits.rs
  - TASK-488
priority: medium
type: bug
ordinal: 511000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
An explicit auto_poll=false setting skips startup import but does not prevent later automatic synchronization. During TASK-488 preview review, background syncing repeatedly restored 500 imported demo commits to the active queue after archival, making the configured behavior and review state unreliable. The desired behavior is that disabling automatic polling is respected consistently, while an explicitly requested manual sync remains available. Define behavior for database-only flakes explicitly and preserve source-history and scheduling semantics.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Automatic synchronization respects a configured auto_poll=false value after startup as well as during initialization
- [ ] #2 Manual synchronization remains available and does not silently enable automatic polling
- [ ] #3 Behavior for database-only flakes is explicit and documented
- [ ] #4 Regression tests prove repeated background cycles do not synchronize an explicitly disabled flake and enabled flakes still synchronize
<!-- AC:END -->
