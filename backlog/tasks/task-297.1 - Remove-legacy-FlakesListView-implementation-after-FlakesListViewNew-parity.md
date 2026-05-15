---
id: TASK-297.1
title: Remove legacy FlakesListView implementation after FlakesListViewNew parity
status: Backlog
assignee: []
created_date: '2026-05-15 15:44'
labels:
  - web-ui
  - flakes
  - cleanup
  - refactor
milestone: UI parity
dependencies: []
references:
  - packages/web-ui/src/views/flakes_list.rs
  - TASK-297
parent_task_id: TASK-297
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: `packages/web-ui/src/views/flakes_list.rs` still contains legacy Flakes list view code paths and helpers that are no longer intended to be used now that FlakesListViewNew is the active implementation. This increases maintenance overhead and causes confusion while iterating on TASK-297.

Desired outcome: remove dead legacy Flakes view code and associated unused helpers in a dedicated cleanup pass, while preserving active behavior and tests for FlakesListViewNew.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Legacy FlakesListView and unused legacy-only helpers are removed
- [ ] #2 Application still routes to FlakesListViewNew without regression
- [ ] #3 `nix develop -c cargo check --target wasm32-unknown-unknown` passes
- [ ] #4 Any follow-up cleanup out of scope is captured in separate backlog tasks
<!-- AC:END -->
