---
id: TASK-321.1
title: Achieve true JSX parity for DashboardView in Rust/Dioxus
status: Backlog
assignee: []
created_date: '2026-06-09 02:12'
labels:
  - ui
  - dashboard
  - parity
  - dioxus
milestone: 'm-6: UI Views - Dashboard'
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/DashboardView.jsx
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/styles.css
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/screens/dash-default.png
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/screens/dash-width.png
modified_files:
  - packages/web-ui/src/views/dashboard.rs
  - packages/web-ui/src/components/widget_grid.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-321
priority: high
ordinal: 3210
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

TASK-321 improved dashboard loading UX and aligned some styling intent, but the Rust/Dioxus dashboard is still not structurally equivalent to `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/DashboardView.jsx` and its associated dashboard screenshots/styles.

Current gaps include:
- 4-column grid instead of the JSX 3-column dense grid
- Different widget set than the JSX 12-widget dashboard
- Extra sections outside the widget grid
- Missing widget-header icon + `View →` action treatment
- Missing customize-mode width/height segmented controls, remove buttons, and widget library modal
- Fleet Health presentation differs from the JSX stacked bar + 4 stat tiles

## Desired Outcome

Implement true visual and structural parity for the Rust/Dioxus dashboard so that it closely matches the JSX reference in layout, widget composition, header treatment, customize interactions, and key widget presentations while preserving production data semantics.
<!-- SECTION:DESCRIPTION:END -->
