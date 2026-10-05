---
id: TASK-481
title: Review reset_non_terminal_derivations behavior for high-attempt derivations
status: Backlog
assignee: []
created_date: '2026-10-05 01:43'
labels:
  - server
  - derivations
dependencies: []
references:
  - docs/knowledge/concepts/derivation-status-lifecycle.md
  - docs/knowledge/meta/cleanup-record.md
  - packages/default/crates/cf-server/src/queries/derivations.rs
priority: low
type: spike
ordinal: 502000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found during the TASK-478 documentation review at commit 3b23d36f.

`reset_non_terminal_derivations` (cf-server queries/derivations.rs, called once from bin/server.rs at start) runs these updates:

1. Any derivation with `attempt_count >= 5` and no `derivation_path` becomes `dry-run-failed` (6), whatever its current status.
2. Any derivation with `attempt_count >= 5` and a `derivation_path` becomes `build-failed` (12), whatever its current status, including `build-complete` (10).
3. Rows with `attempt_count < 5` and a status other than 5 or 10 return to `dry-run-pending` (3) or `build-pending` (7). This includes terminal statuses such as 6, 11, 12, 13, and 14.

Only the legacy server-side build path (build_reservations) increments `attempt_count`, so the API-builder path rarely triggers it. The function name and the in-code comment say "non-terminal", but the SQL does not filter on terminal status. The knowledge base describes the actual behavior in docs/knowledge/concepts/derivation-status-lifecycle.md.

Decide whether this is intended. If it is not, correct the predicates.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Document whether each of the three updates is intended, with the reason.
- [ ] #2 If any update is a defect, add a regression test that proves a build-complete derivation keeps its status across a server restart, and fix the predicates.
- [ ] #3 Update docs/knowledge/concepts/derivation-status-lifecycle.md and the cleanup record gap G-008 to match the decision.
<!-- AC:END -->
