---
id: TASK-275
title: Refactor builds and evaluations views for visual coherence and density
status: To Do
assignee: []
created_date: '2026-04-18 01:56'
updated_date: '2026-05-31 16:07'
labels:
  - ui
  - ux
  - refactor
  - builds
  - evaluations
  - dioxus
milestone: m-16
dependencies:
  - TASK-328
  - TASK-329
  - TASK-333
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Builds and Evaluations views present similar operational workflows but currently diverge in layout, interaction rhythm, and density, reducing usability and design consistency.

## Goal
Unify both views under a coherent, high-density interaction model aligned to CrystalForgelatest design standards while preserving existing operational functionality.

## Non-Goals
- No new backend workflow semantics outside existing feature scope.
- No non-view-layer refactors unrelated to builds/evaluations surfaces.
- No speculative feature additions not represented in design direction.

## Scope
- Align page structure, tab patterns, active/history segmentation, and detail-pane behavior.
- Standardize table-centric queue presentation and row-selection behavior.
- Preserve current queue actions, controls, and websocket/live updates.
- Harmonize visual tokens, spacing, and typography with milestone parity standards.

## Architectural Constraints
- Keep state/data adapters separate from presentational view components.
- Avoid business logic in rendering modules.
- Reuse shared table/filter primitives where possible.

## Verification Plan
- `AUTH_MODE=dev nix run .#web-ui.dx-serve` for iterative visual validation.
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
- `nix build .#checks.x86_64-linux.web-ui`
- Update `checks/web-ui` with assertion-based interaction coverage for builds and evaluations.
- Capture screenshot coverage for loading/empty/error/populated and key tab/detail states.

## Impact Areas
- `packages/web-ui/src/views/builds.rs`
- `packages/web-ui/src/views/evaluations.rs`
- Related components under `packages/web-ui/src/components/**`
- `checks/web-ui/**`

## Risk Level
High (high-traffic operational screens with many interaction paths).

## Dependencies
- Design parity matrix TASK-328.
- Shared token/shell alignment TASK-329.
- Verification harness TASK-333.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both builds and evaluations views have consistent tab navigation (Active Queue / History or Completed)
- [ ] #2 Active queue displays as a table (not cards) with columns for status, system/config, commit, and time
- [ ] #3 Table rows are selectable and highlight when clicked
- [ ] #4 Selecting a queue item auto-populates the detail pane on the right
- [ ] #5 First row auto-selects on page load when queue is not empty
- [ ] #6 Detail pane shows relevant information (logs, metadata, actions) appropriate to the view
- [ ] #7 History/Completed tab shows full-width table with sorting and filtering
- [ ] #8 All existing functionality is preserved (worker controls, queue actions, drag-reorder for evals, websocket logs)
- [ ] #9 Visual design is coherent between the two views (similar spacing, typography, color usage)
- [ ] #10 Information density is improved - key data is visible without excessive scrolling or clicking
- [ ] #11 Search and filter controls work and are consistently positioned
- [ ] #12 Running AUTH_MODE=dev nix run .#web-ui.dx-serve provides hot-reload during development
- [ ] #13 web-ui check must include assertion-based verification for builds/evaluations interaction correctness
- [ ] #14 web-ui check must include screenshots for all targeted builds/evaluations parity states
<!-- AC:END -->
