---
id: TASK-372
title: 'Fix live refresh behavior on Builds, Evaluations, and Systems views'
status: To Do
assignee: []
created_date: '2026-06-27 03:56'
labels:
  - web-ui
  - live-refresh
  - builds
  - evaluations
  - systems
  - ux
  - bug
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - packages/web-ui/src/views/builds.rs
  - packages/web-ui/src/views/evaluations.rs
  - packages/web-ui/src/views/systems.rs
  - packages/web-ui/src/api/client.rs
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Several views advertise live data with UI such as `Live · updated 5s ago`, but when the timer reaches the refresh interval/reset point (around 60s), the visible data does not appear to refresh. This makes the live indicator misleading: the timestamp resets, but the underlying Builds/Evaluations data remains stale. The Systems view has a related but more granular need: heartbeat timers should update per system card as each system receives a heartbeat, rather than requiring a full-page/manual refresh.

## Goal

Make the live indicators truthful by ensuring the affected views actually refresh their data when the live timer cycles, and make Systems heartbeat freshness update at the per-system/card level.

## Non-Goals

- Redesigning Builds, Evaluations, or Systems layouts
- Replacing all polling with WebSockets/SSE in this task
- Changing backend heartbeat semantics
- Adding new metrics or dashboard features beyond fixing refresh behavior
- Refactoring unrelated view state management

## Architectural Constraints

- Keep refresh orchestration in view/controller/state logic, not presentation-only components.
- Do not introduce hidden global mutable state.
- Prefer existing API client/view data-loading patterns.
- Avoid excessive polling or thundering-herd refresh behavior.
- Systems heartbeat freshness should update per system where practical, without unnecessarily refetching unrelated view data.
- Preserve existing filters, pagination, sorting, and selection state across live refreshes.
- Follow existing Dioxus signal/resource patterns used in the web UI.

## Impact Areas

- Builds view live indicator and data reload loop
- Evaluations view live indicator and data reload loop, if it shares the same live-refresh bug/pattern
- Systems view heartbeat freshness display and per-system card update behavior
- Shared live indicator/timer component or hook, if one exists
- Web UI API client calls used by these views

## Risk Level

Medium: live refresh touches state lifecycles and can accidentally reset user context, overfetch, or regress filters/pagination/selection. Mitigate with scoped tests/manual checks for refresh behavior and state preservation.

## Verification Plan

- Identify whether Builds/Evaluations/Systems share a live refresh helper or have independent timer logic.
- Add targeted unit/component tests if the refresh trigger is isolated and testable.
- Add/update browser/UI check if practical to assert that a refresh callback/API reload occurs after the live interval.
- Manual verification:
  - Builds view data reloads when `Live · updated …` reaches the refresh interval/reset point.
  - Evaluations view is checked and fixed if it has the same stale-refresh behavior.
  - Existing Builds/Evaluations filters, pagination, and selection state survive refresh.
  - Systems heartbeat freshness updates per system card when heartbeat data changes.
  - No global text/layout refresh flicker or full state reset is introduced.
- Because this is a UI behavior change, include screenshot or recording evidence from the `web-ui` check/MR verification.
- Run appropriate frontend verification, for example:
  - `nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check` or changed-file rustfmt equivalent
  - `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
  - relevant `web-ui` check if needed for UI screenshot/behavior assertion

## Proposed Approach

1. Locate the live indicator/timer implementation in Builds, Evaluations, and Systems views and determine whether it currently only resets display time without triggering data reload.
2. Introduce or fix a refresh tick signal/resource invalidation so the timer interval triggers actual data fetching.
3. Preserve user view state during refresh: filters, sorting, pagination, selected items, expanded rows/cards, and current tab where applicable.
4. For Systems, update heartbeat-related freshness at the system-card level by refreshing/merging system rows or heartbeat fields rather than blindly resetting the whole view.
5. If a shared live-refresh helper exists, fix it there; otherwise apply the smallest scoped fixes to each affected view.
6. Add targeted tests or document manual/browser verification for the live refresh path.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Builds view performs an actual data refresh when the live updated timer reaches its refresh/reset interval.
- [ ] #2 Evaluations view is audited for the same live-refresh bug and fixed if affected.
- [ ] #3 Systems view heartbeat freshness updates per system card as heartbeat data changes, without requiring a full manual page refresh.
- [ ] #4 Live refresh preserves existing user context such as filters, sorting, pagination, selected rows/items, and expanded UI state where applicable.
- [ ] #5 The live indicator no longer resets independently of the underlying data refresh.
- [ ] #6 Refresh behavior avoids excessive polling and does not introduce duplicate concurrent fetch loops.
- [ ] #7 Manual verification or targeted UI/browser tests cover Builds live refresh, Evaluations if affected, and per-system heartbeat refresh behavior.
<!-- AC:END -->
