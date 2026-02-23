---
id: TASK-124
title: Environments View - Backend Integration
status: In Progress
assignee: []
created_date: '2026-02-23'
updated_date: '2026-02-23 22:54'
labels:
  - backend
  - api
  - web-ui
  - environments
milestone: m-9
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Environments View: Backend Integration
Problem

Environments are currently mocked in UI.

Goal

Expose environments dynamically from backend with correct scoping.

Backend Scope
Endpoints
GET /api/environments
GET /api/environments/:id
Example Response
{
  "environments": [
    {
      "id": "env-prod",
      "name": "prod",
      "system_count": 6
    }
  ]
}
Requirements

User only sees environments they belong to.

Server enforces membership.

No client-side filtering for security.

Frontend Scope
environments/
  api.rs
  models.rs
  adapter.rs
  view.rs

Same adapter fallback pattern.

Environment list drives filtering in other views.

Acceptance Criteria

Real environments render.

Scoping enforced.

Fallback mock data works.

No duplication of policy logic in UI.

Risk Level

Low-Medium
<!-- SECTION:DESCRIPTION:END -->

## Problem Statement

Environments are currently mocked in UI. There is no real API backing for:
- Listing environments
- Environment details and system counts
- User-specific environment visibility

---

## Goal

Expose environments dynamically from backend with correct scoping.

---

## Non-Goals

- Implementing environment create/update operations
- Changing environment visualization UI significantly

---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 #1 Backend GET /api/environments endpoint implemented
- [ ] #2 #2 Backend GET /api/environments/:id endpoint implemented
- [ ] #3 #3 User only sees environments they belong to (server enforcement)
- [ ] #4 #4 No client-side filtering for security
- [ ] #5 #5 Frontend environments/api.rs created
- [ ] #6 #6 Frontend environments/models.rs created
- [ ] #7 #7 Frontend environments/adapter.rs created with fallback logic
- [ ] #8 #8 Frontend environments/view.rs updated to use adapter
- [ ] #9 #9 Environment list drives filtering in other views
- [ ] #10 #10 401/403 redirects to login
- [ ] #11 #11 500/network errors fallback to mock data
- [ ] #12 #12 Verification commands pass

---

## Architectural Constraints

- No business logic in UI views
- All DTOs defined in frontend models.rs
- All HTTP calls isolated in api.rs
- All fallback logic isolated in adapter.rs
- No network calls directly inside view components
- Server enforces RBAC - no client-side filtering for authorization

---

## Verification Plan

Automated:

```
nix build .#checks.x86_64-linux.default
nix build .#checks.x86_64-linux.web-ui
nix develop -c cargo test --package web-ui environments
```

Manual:
- Navigate to Environments view and verify real data loads
- Verify user only sees their assigned environments
- Test that environment selection drives filtering in other views
- Verify fallback to mock data when backend unavailable

---

## Impact Areas

- Backend API
- Web UI
- RBAC enforcement

---

## Risk Level

Low-Medium

---

## Dependencies

- TASK-121 (Systems View Backend Integration)

---

## Follow-Up Tasks (if discovered during grooming)

- Add unit tests for environments adapter
- Implement environment create/update operations
- Add environment health monitoring
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-sonnet-4-6 on gray in ~/code/crystal-forge/TASK-124-environments-backend-integration
<!-- SECTION:NOTES:END -->
