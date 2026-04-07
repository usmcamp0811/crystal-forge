---
id: TASK-228
title: Fix Fleet Health widget mismatch with Systems health statuses
status: Review
assignee: []
created_date: '2026-03-30 03:04'
updated_date: '2026-04-07 02:21'
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
- [x] #1 Given at least one system is `critical` in Systems view, Fleet Health widget must show a non-zero critical count.
- [x] #2 Fleet Health healthy/degraded/critical totals on dashboard must match Systems view status distribution for the same environment/filter scope.
- [x] #3 Health rollup uses the same source-of-truth status semantics as Systems view (documented in task notes).
- [x] #4 Regression test coverage exists for a mixed fleet containing healthy and critical systems.
- [x] #5 Given one system is offline, Fleet Health widget must report offline/non-healthy accurately and must not count that system as healthy.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-04-06 production evidence: Systems view shows 13 healthy + 1 offline, but Fleet Health widget shows 14 healthy. Widget aggregation is overcounting healthy and ignoring offline bucket/state.

Promoted to To Do per maintainer instruction to begin work.

LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-228-fleet-health-widget-counts

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/213

Implementation: `fetch_fleet_health` now aggregates from `view_system_list.health_status` (same source used by Systems view) and maps statuses case-insensitively into healthy/warning/critical/offline buckets.

Verification run: `nix develop -c bash -lc "SQLX_OFFLINE=true cargo test --lib queries::dashboard::tests"` (pass), `nix develop -c bash -lc "SQLX_OFFLINE=true cargo check"` (pass), `nix develop -c cargo fmt --all -- --check` (fails due unrelated pre-existing formatting diffs), `nix develop -c rustfmt --edition 2024 --check src/queries/dashboard.rs` (pass for touched file).

Follow-up per maintainer request: added `web-ui` integration assertion step `06z-fleet-health-widget-assert` in `checks/web-ui/tests/integration-test.js` to verify Fleet Health legend counts (healthy/warning/critical/offline) from mocked mixed-status dashboard data.

Updated `checks/web-ui/default.nix` critical test lists to require `06z-fleet-health-widget-assert` in both ci_fast and full profiles so regressions fail the check.

Verification: `node --check checks/web-ui/tests/integration-test.js` (pass), `nix build .#checks.x86_64-linux.web-ui` (pass).
<!-- SECTION:NOTES:END -->
