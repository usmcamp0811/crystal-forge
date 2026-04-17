---
id: TASK-201
title: Dashboard Fleet Health widget showing mock data instead of real systems
status: Done
assignee: []
created_date: '2026-03-20 13:40'
updated_date: '2026-03-31 01:56'
labels:
  - frontend
  - dashboard
  - bug
  - high-priority
dependencies: []
references:
  - packages/web-ui/src/views/dashboard.rs
  - packages/web-ui/src/components/dashboard/fleet_health.rs
  - packages/default/src/queries/dashboard.rs
priority: high
ordinal: 1200
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The Dashboard Fleet Health widget is showing mock/demo values instead of reflecting real system health data, which makes the dashboard misleading for operators.

## Goal

Wire Fleet Health to real backend dashboard/system data so the widget displays accurate live counts and percentages for system health states.

## Non-Goals

- No visual redesign of dashboard layout or card styling.
- No changes to unrelated dashboard widgets.
- No backend schema/migration changes.
- No new auth/permission model changes.

## Architectural Constraints

- Keep data-fetch orchestration in dashboard view/state layer, not in pure presentational components.
- Keep Fleet Health component presentational and reusable.
- Follow existing API model contracts used by other dashboard widgets.
- Avoid introducing global mutable state.

## Verification Plan

- Confirm Fleet Health widget values are sourced from API response, not mock constants.
- Validate loading/error/empty states behave correctly.
- Run targeted web-ui build/check for dashboard path and full `web-ui` integration check.

## Impact Areas

- `packages/web-ui/src/views/dashboard.rs`
- `packages/web-ui/src/components/dashboard/fleet_health.rs`
- Optional: shared dashboard API/model mapping files if required by existing pattern.

## Risk Level

Medium (operator-facing correctness issue on main dashboard).
<!-- SECTION:DESCRIPTION:END -->

# Dashboard Fleet Health widget showing mock data instead of real systems

---

# Problem Statement

The Fleet Health widget on the Dashboard view displays hardcoded mock data (e.g., "server-01") instead of actual system health data from the database. This makes the dashboard misleading and prevents users from seeing real fleet status.

---

# Goal

Fleet Health widget displays real system data from the database, including actual system hostnames, health status, agent connectivity, and build/deployment status.

---

# Non-Goals

- Redesigning the Fleet Health widget UI
- Adding new health metrics or checks
- Implementing WebSocket real-time updates (use existing pattern)
- Changing dashboard layout or widget sizing
- Adding filtering by environment (separate task)

---

# Acceptance Criteria

- [ ] Fleet Health widget queries real data via `/api/dashboard` endpoint
- [ ] Widget displays actual system hostnames from database
- [ ] Widget shows accurate health status for each system:
  - Healthy (agent connected, recent successful deployment)
  - Warning (agent connected, deployment issues)
  - Critical (agent disconnected or system errors)
- [ ] Widget shows agent connectivity status (connected/disconnected)
- [ ] Empty state shown gracefully when no systems registered
- [ ] Mock data removed from frontend code
- [ ] Backend query uses real `systems` table data
- [ ] Data fetching follows existing dashboard pattern (spawn + async fetch)
- [ ] Loading state shown while fetching data
- [ ] Error state shown on fetch failure

---

# Architectural Constraints

- Follow existing dashboard data fetching pattern (see dashboard.rs line 159, 182)
- Use existing `/api/dashboard` endpoint or extend it
- Backend queries in `queries/dashboard.rs` module
- No hardcoded mock data in production code
- UI components in `components/dashboard/` directory
- Use existing health status types/enums
- No schema changes (use existing systems table)

---

# Verification Plan

Automated:
- `nix develop -c cargo test queries::dashboard`
- `nix develop -c cargo clippy -- -D warnings`
- `nix develop -c cargo fmt -- --check`
- UI build: `nix build .#web-ui`

Manual:
- Start dev stack with registered systems
- Navigate to Dashboard
- Verify Fleet Health widget shows real system names
- Verify health status indicators match actual system state
- Register a new system
  - Refresh dashboard
  - Verify new system appears in Fleet Health widget
- Disconnect an agent
  - Verify widget shows disconnected status
- Test with zero systems registered
  - Verify empty state message shown
- Check browser console for errors

---

# Impact Areas

UI | API | Domain

- Dashboard view (frontend)
- Fleet Health widget component
- `/api/dashboard` endpoint
- `queries/dashboard::fetch_fleet_health`
- System health calculation logic

---

# Risk Level

Low

This is primarily a bug fix replacing mock data with real data. Existing endpoint and query structure should support this. Risk is limited to:
- Query performance if many systems (mitigate with LIMIT/pagination)
- UI breaking if data shape doesn't match expectations

Mitigations:
- Use existing proven dashboard data fetching pattern
- Add proper error handling and empty states
- Test with various system counts (0, 1, 10)

---

# Dependencies

None

---

# Follow-Up Tasks

- Add pagination/grouping for fleet health if >20 systems
- Add filtering by environment to Fleet Health widget
- Add drill-down to system detail from Fleet Health widget
- Add configurable health status thresholds

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Fleet Health widget on Dashboard reads from real backend data and no longer displays hardcoded/mock values.
- [ ] #2 Displayed counts/percentages match current system health states from API response in the same session.
- [ ] #3 Widget shows sensible loading and error states without falling back to fake data.
- [ ] #4 Dashboard remains responsive and no regressions are introduced in adjacent widgets.
- [ ] #5 `nix build .#checks.x86_64-linux.web-ui` passes with Fleet Health behavior validated.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Inspect dashboard view wiring and identify where Fleet Health currently receives mock data.
2. Replace mock wiring with real API-backed data mapping using existing dashboard/system summary response.
3. Ensure Fleet Health widget handles loading, error, and no-data states consistently with surrounding dashboard widgets.
4. Add or adjust focused UI regression coverage for Fleet Health real-data rendering path.
5. Run verification commands and prepare MR notes with before/after behavior.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Sprint-ready execution notes:
- Prioritize minimal-scope bug fix with no redesign.
- Preserve existing component boundaries (container fetch logic vs presentational card).
- If additional data fields are needed, prefer extending existing dashboard response mapping before introducing new API surface.
- Create follow-up backlog tasks for any out-of-scope findings discovered during implementation.

LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-201-fleet-health-real-data

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/193

Commit: ae54c542
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Implementation is scoped to Fleet Health data wiring and related mapping only.
- [ ] #2 MR description includes verification evidence that values are real-data driven (not mock).
- [ ] #3 Any discovered follow-up improvements are captured as separate Backlog tasks.
<!-- DOD:END -->
