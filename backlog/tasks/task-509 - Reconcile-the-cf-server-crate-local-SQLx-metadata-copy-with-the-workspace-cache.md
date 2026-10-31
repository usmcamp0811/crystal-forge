---
id: TASK-509
title: >-
  Reconcile the cf-server crate-local SQLx metadata copy with the workspace
  cache
status: Backlog
assignee: []
created_date: '2026-10-31T21:36:00Z'
labels: []
dependencies: []
priority: low
type: chore
ordinal: 560000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found during TASK-508 verification. `cargo sqlx prepare --check` passes for the workspace cache `packages/default/.sqlx` (used by the Nix server build) but fails for the compatibility copy `packages/default/crates/cf-server/.sqlx` with ".sqlx is missing one or more queries". At dev 18b47eed the crate copy already lacks five query files present in the workspace cache (e.g. query-4a0b55ae..., query-4d21dd5c..., query-523e9fb4..., query-5b9de612..., query-a1c07d79...). TASK-508 did not change either cache. A stale copy misleads anyone preparing metadata from the crate directory.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `cargo sqlx prepare --check` passes in both packages/default and packages/default/crates/cf-server against a freshly migrated isolated database, or the redundant copy is removed with its consumers updated
- [ ] #2 Documentation describing the two caches matches the result
<!-- AC:END -->
