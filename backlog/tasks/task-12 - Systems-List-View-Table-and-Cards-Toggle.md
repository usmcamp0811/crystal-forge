---
id: TASK-12
title: Systems List View - Table and Cards Toggle
status: To Do
assignee: []
created_date: '2026-02-05 14:25'
labels:
  - ui
  - views
  - systems
dependencies:
  - TASK-8.7
priority: high
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build systems list with toggleable table/cards view.

Steps:
1. Create src/views/systems.rs
2. Fetch systems list from MockClient
3. Implement table view with sortable columns
4. Implement cards grid view
5. Add toggle button to switch views
6. Add filters: environment, health, deployment status
7. Add search bar
8. Save view preference to local storage

Expected: Users can switch between views easily
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Table view implemented
- [ ] #2 Cards view implemented
- [ ] #3 Toggle works
- [ ] #4 Filters functional
- [ ] #5 Search works
- [ ] #6 Preference persists
<!-- AC:END -->
