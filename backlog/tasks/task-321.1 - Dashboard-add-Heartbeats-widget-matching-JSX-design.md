---
id: TASK-321.1
title: 'Dashboard: add Heartbeats widget matching JSX design'
status: Backlog
assignee: []
created_date: '2026-06-09 02:42'
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
parent_task_id: TASK-321
priority: medium
ordinal: 289000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The JSX design reference (`CrystalForgelatest/components/DashboardView.jsx`, `WHeartbeat`) includes a "Heartbeats" dashboard widget that is not present in the Rust/Dioxus dashboard delivered in TASK-321.

## Desired Outcome
Add a `heartbeat-status` widget to the dashboard widget registry that visually matches the JSX `WHeartbeat`:
- Big "overdue" count (amber when > 0, green otherwise)
- Optional red banner: "N systems past 2x heartbeat interval"
- Footer: "H of T reporting on schedule"
- Header icon `warn`, title "Heartbeats", "View ->" navigates to systems

Back it with real heartbeat data from the systems API (heartbeat interval / next-in / overdue), not mock constants.

## Notes
- Widget registry + grid shell already exist (TASK-321); this adds one widget entry + content renderer.
- Reference: WHeartbeat lines ~250-271 of DashboardView.jsx.
<!-- SECTION:DESCRIPTION:END -->
