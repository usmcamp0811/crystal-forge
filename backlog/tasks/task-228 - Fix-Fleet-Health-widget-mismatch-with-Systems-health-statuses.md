---
id: TASK-228
title: Fix Fleet Health widget mismatch with Systems health statuses
status: Backlog
assignee: []
created_date: '2026-03-30 03:04'
updated_date: '2026-04-07 00:45'
labels:
  - dashboard
  - health
  - bug
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The Dashboard Fleet Health widget reports all 14 systems as healthy, but the Systems view currently includes at least one system with `critical` health. This creates an inconsistent operational picture between dashboard summary and per-system truth.

## Desired Outcome
Fleet Health widget counts and severity buckets match the same health classification source used by the Systems view, so any `critical` system is reflected immediately in the dashboard summary.

## Scope Notes
- Investigate data source and aggregation path used by the dashboard widget.
- Align health rollup logic with Systems view health status semantics.
- Add regression coverage for mixed-health fleets (including at least one critical system).
- Keep changes scoped to dashboard health computation and related query/API wiring only.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Given at least one system is `critical` in Systems view, Fleet Health widget must show a non-zero critical count.
- [ ] #2 Fleet Health healthy/degraded/critical totals on dashboard must match Systems view status distribution for the same environment/filter scope.
- [ ] #3 Health rollup uses the same source-of-truth status semantics as Systems view (documented in task notes).
- [ ] #4 Regression test coverage exists for a mixed fleet containing healthy and critical systems.
- [ ] #5 Given one system is offline, Fleet Health widget must report offline/non-healthy accurately and must not count that system as healthy.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-04-06 production evidence: Systems view shows 13 healthy + 1 offline, but Fleet Health widget shows 14 healthy. Widget aggregation is overcounting healthy and ignoring offline bucket/state.
<!-- SECTION:NOTES:END -->
