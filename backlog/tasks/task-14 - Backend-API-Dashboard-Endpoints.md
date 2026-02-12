---
id: TASK-14
title: Backend API - Dashboard Endpoints
status: To Do
assignee: []
created_date: '2026-02-05 14:25'
labels:
  - backend
  - api
dependencies: []
priority: high
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement server-side API endpoints for dashboard.

Steps:
1. Create src/handlers/dashboard.rs in main Crystal Forge server
2. Implement GET /api/v1/dashboard/summary
3. Query database for system counts by health status
4. Aggregate CVE counts across fleet
5. Count active builds and pending approvals
6. Return JSON response matching DashboardSummary model
7. Add error handling
8. Write integration tests

Expected: Dashboard API returns real data
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Dashboard summary endpoint works
- [ ] #2 Queries optimized
- [ ] #3 Error handling complete
- [ ] #4 Tests pass
<!-- AC:END -->
