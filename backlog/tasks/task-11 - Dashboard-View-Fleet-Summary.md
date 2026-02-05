---
id: TASK-11
title: Dashboard View - Fleet Summary
status: To Do
assignee: []
created_date: '2026-02-05 14:25'
labels:
  - ui
  - views
  - dashboard
dependencies:
  - TASK-8.7
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement main dashboard view with fleet-wide metrics.

Steps:
1. Create src/views/dashboard.rs
2. Use MockClient to fetch dashboard summary
3. Display metric cards: total systems, healthy, degraded, offline
4. Show CVE summary with donut chart
5. Display recent deployments timeline
6. Add real-time updates via WebSocket (placeholder)
7. Make responsive for mobile

Expected: Dashboard shows all key metrics
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Dashboard view renders
- [ ] #2 Metrics displayed correctly
- [ ] #3 CVE chart works
- [ ] #4 Timeline shows deployments
- [ ] #5 Responsive layout
<!-- AC:END -->
