---
id: TASK-15
title: Backend API - Systems Endpoints
status: To Do
assignee: []
created_date: '2026-02-05 14:25'
labels:
  - backend
  - api
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement systems management API endpoints.

Steps:
1. Create src/handlers/systems.rs
2. Implement GET /api/v1/systems with filtering
3. Implement GET /api/v1/systems/:id for details
4. Implement POST /api/v1/systems/:id/deploy
5. Implement POST /api/v1/systems/:id/rollback
6. Add query parameter support for filters (environment, health)
7. Add pagination for large fleets
8. Write integration tests

Expected: All system operations work via API
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 List endpoint with filters
- [ ] #2 Detail endpoint complete
- [ ] #3 Deploy endpoint works
- [ ] #4 Rollback endpoint works
- [ ] #5 Pagination implemented
- [ ] #6 Tests pass
<!-- AC:END -->
