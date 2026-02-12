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
  - TASK-8.8
  - TASK-9
priority: high
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement main dashboard view with fleet-wide metrics using Tailwind CSS.

Steps:
1. Create src/ui/views/dashboard.rs
2. Use MockClient (or real API client) to fetch dashboard summary
3. Display metric cards: total systems, healthy, warning, critical, offline
4. Show CVE summary using colored counters/progress bars (donut chart is a stretch goal)
5. Display recent deployments as a simple list/timeline (most recent first)
6. Implement data fetching with loading and error states (via TASK-8.8 state management)
7. Make responsive for mobile using Tailwind grid/flex
8. Add periodic polling (30s interval) for near-real-time updates (WebSocket deferred)

Architecture notes:
- Donut chart is a **stretch goal** - use simple counters with colored badges for v1
- WebSocket real-time updates **deferred** to future milestone - use polling for now
- Dashboard data comes from GET /api/v1/dashboard/summary (TASK-14)

Expected: Dashboard shows all key fleet metrics with dark theme, responsive layout
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Dashboard view renders with fleet metrics
- [ ] #2 System health counts displayed (healthy/warning/critical/offline)
- [ ] #3 CVE summary displayed with severity-colored counters
- [ ] #4 Recent deployments listed
- [ ] #5 Responsive layout (mobile + desktop)
- [ ] #6 Loading and error states handled
- [ ] #7 (Stretch) Donut chart for CVE breakdown
<!-- AC:END -->
