---
id: TASK-297.1
title: Remove legacy FlakesListView implementation after FlakesListViewNew parity
status: Backlog
assignee: []
created_date: '2026-05-15 15:44'
updated_date: '2026-05-31 16:04'
labels:
  - web-ui
  - flakes
  - cleanup
  - refactor
milestone: m-16
dependencies:
  - TASK-297
  - TASK-328
  - TASK-333
references:
  - packages/web-ui/src/views/flakes_list.rs
  - TASK-297
parent_task_id: TASK-297
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Legacy `FlakesListView` code paths remain in the codebase after parity migration to `FlakesListViewNew`, increasing maintenance burden and creating risk of accidental divergence.

## Goal
Remove legacy flakes list implementation safely after parity and verification are proven on the active `FlakesListViewNew` path.

## Non-Goals
- No redesign of flakes UX beyond removing dead legacy paths.
- No backend API contract changes.
- No unrelated refactors in non-flakes views.

## Scope
- Remove legacy `FlakesListView` components/helpers that are no longer used.
- Confirm route wiring points only to `FlakesListViewNew`.
- Clean up imports/types/tests tied only to legacy implementation.
- Ensure parity behavior remains unchanged after cleanup.

## Architectural Constraints
- Preserve separation between view rendering and data-mapping logic.
- Avoid introducing new shared abstractions unless required for deletion safety.
- Keep cleanup incremental and traceable.

## Verification Plan
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
- `nix build .#checks.x86_64-linux.web-ui`
- Update `checks/web-ui` with flakes parity assertions before/after legacy removal.
- Capture screenshots for all affected flakes states before and after switch.

## Impact Areas
- `packages/web-ui/src/views/flakes_list.rs`
- Related flakes components/helpers under `packages/web-ui/src/components/**`
- `checks/web-ui/**`

## Risk Level
Medium (cleanup task with potential regression if old code still referenced indirectly).

## Dependencies
- Parent parity delivery TASK-297 and milestone parity spec TASK-328.
- Verification harness coverage TASK-333.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Legacy FlakesListView and unused legacy-only helpers are removed
- [ ] #2 Application still routes to FlakesListViewNew without regression
- [ ] #3 `nix develop -c cargo check --target wasm32-unknown-unknown` passes
- [ ] #4 Any follow-up cleanup out of scope is captured in separate backlog tasks
- [ ] #5 web-ui check must assert flakes-view behavior parity before legacy view removal
- [ ] #6 web-ui check must include full screenshots for affected flakes-view states before and after switch
<!-- AC:END -->
