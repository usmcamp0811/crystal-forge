---
id: TASK-342.1
title: 'Dashboard: remove mock/fallback data from production render path'
status: To Do
assignee: []
created_date: '2026-06-10 13:30'
updated_date: '2026-06-11 12:39'
labels:
  - design-parity
  - dashboard
  - api-integration
  - child
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-342
  - packages/web-ui/src/views/dashboard.rs
  - packages/web-ui/src/dashboard/adapter.rs
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/dashboard.rs
  - packages/web-ui/src/dashboard/adapter.rs
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-342
priority: high
ordinal: 1731
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Dashboard umbrella TASK-342. Follow guide doc-14 standard procedure.

## Problem
`packages/web-ui/src/views/dashboard.rs` imports `load_dashboard_with_fallback`, `empty_dashboard_summary`, `deterministic_mock_timestamp`, and `load_flake_timelines_with_fallback` from `dashboard::adapter`. The production path must render real API data only, with proper loading/empty/error states.

## Goal
Ensure all dashboard widgets render from real backend data in production; remove mock timestamps and fallback summaries from the production path.

## Exact scope
1. Replace `load_dashboard_with_fallback` / `load_flake_timelines_with_fallback` production usage with real API calls plus real loading/empty/error states.
2. Remove `deterministic_mock_timestamp` usage from production rendering.
3. Keep `empty_dashboard_summary` only as a genuine empty-state (not as fabricated data) — verify it renders zeros/empty, not fake values.
4. Confirm each widget (FleetHealth, BuildQueue, BuildSummary, CveSummary, DeploymentStatus, RecentDeployments, FlakeTimeline) shows real data or a real empty state.

## Non-goals
- No widget layout redesign (sibling task).
- No backend endpoint changes unless a field is missing (note for TASK-332).

## Files
- packages/web-ui/src/views/dashboard.rs
- packages/web-ui/src/dashboard/adapter.rs
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Extend step `06-dashboard` (and `06z-fleet-health-widget-assert`) to assert no fabricated values render on API error/empty.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Dashboard widgets render from real API data in the production path
- [ ] #2 Mock timestamp helper is not used in production rendering
- [ ] #3 Empty state renders genuine empty/zero values, not fabricated data
- [ ] #4 API error renders a real error state rather than mock data
- [ ] #5 web-ui step asserts no fabricated dashboard values render on error/empty
<!-- AC:END -->
