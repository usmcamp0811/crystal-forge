---
id: TASK-342.2
title: 'Dashboard: widget grid layout + widget visuals parity'
status: To Do
assignee: []
created_date: '2026-06-10 13:30'
updated_date: '2026-06-11 20:16'
labels:
  - design-parity
  - dashboard
  - web-ui
  - child
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-342
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/DashboardView.jsx
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/dashboard.rs
  - packages/web-ui/src/components/dashboard/mod.rs
  - packages/web-ui/src/components/widget_grid.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-342
priority: high
ordinal: 1732
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Dashboard umbrella TASK-342. Follow guide doc-14 standard procedure.

## Problem
The dashboard widget grid and individual widget visuals must match `CrystalForgelatest/components/DashboardView.jsx` within doc-8 tolerances.

## Goal
Pixel-align the dashboard grid arrangement, widget headers, spacing, and the fleet-health/CVE/build/eval widget internals to the design.

## Exact scope
1. Grid: column count, gaps, and widget sizing match the design's dense grid.
2. Widget headers: title/typography/spacing match design.
3. Fleet Health: stacked-bar + stat-tile layout per design.
4. Build Queue / CVE Summary / Eval / Recent Deployments / Flake Timeline internals match design.
5. Loading spinner state matches design treatment (already partially present via DashboardLoadingSpinner).

## Non-goals
- Mock removal (sibling task TASK-342.x).
- Customize-mode behavior changes beyond visual parity (already implemented in TASK-321).

## Files
- packages/web-ui/src/views/dashboard.rs
- packages/web-ui/src/components/dashboard/** 
- packages/web-ui/src/components/widget_grid.rs
- packages/web-ui/assets/app.css
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Extend steps `06-dashboard`, `06-dashboard-loading-spinner`, `06z-fleet-health-widget-assert` for layout/visual coverage.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Widget grid columns/gaps/sizing match the design within doc-8 tolerances
- [ ] #2 Widget headers and spacing match the design
- [ ] #3 Fleet Health renders the stacked-bar + stat-tile design
- [ ] #4 Loading spinner state matches the design treatment
- [ ] #5 web-ui step screenshots the populated dashboard and asserts the fleet-health widget
<!-- AC:END -->
