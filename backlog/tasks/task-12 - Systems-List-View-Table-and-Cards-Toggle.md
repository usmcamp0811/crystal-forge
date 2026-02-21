---
id: TASK-12
title: Systems List View - Table and Cards Toggle
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-05 14:25'
updated_date: '2026-02-21 03:28'
labels:
  - ui
  - views
  - systems
milestone: m-7
dependencies:
  - TASK-8.7
  - TASK-8.8
  - TASK-9
  - TASK-10
priority: high
ordinal: 38000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build systems list with toggleable table/cards view using Tailwind CSS.

Steps:
1. Create src/ui/views/systems_list.rs
2. Fetch systems list from MockClient or real API (GET /api/v1/systems)
3. Implement table view with sortable columns (hostname, env, health, deployment, CVEs)
4. Implement cards grid view using SystemCard component (TASK-10)
5. Add toggle button (table/grid icon) to switch between views
6. Add filters: environment dropdown, health status, deployment status
7. Add search bar for hostname filtering
8. Save view preference to browser local storage via gloo-storage
9. Style with Tailwind dark theme (table: bg-gray-800, striped rows, hover states)

Architecture notes:
- Local storage for WASM requires `gloo-storage` crate
- Filter state managed via local Dioxus signals
- Systems list state from global context (TASK-8.8)

Expected: Users can switch between table and card views, filter and search systems
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Table view implemented
- [x] #2 Cards view implemented
- [x] #3 Toggle works
- [x] #4 Filters functional
- [x] #5 Search works
- [x] #6 Preference persists
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Completed implementation with Playwright UI test automation verifying table/cards toggle works correctly.
<!-- SECTION:NOTES:END -->
