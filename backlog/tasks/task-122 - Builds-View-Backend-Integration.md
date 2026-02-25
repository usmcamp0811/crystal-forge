---
id: TASK-122
title: Builds View - Backend Integration
status: Done
assignee: []
created_date: '2026-02-23'
updated_date: '2026-02-25 00:53'
labels:
  - backend
  - api
  - web-ui
  - builds
milestone: m-11
dependencies: []
priority: high
ordinal: 77000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Builds View: Backend Integration
Problem

Builds view currently uses static/mock build history.

Goal

Implement full backend integration for build history and status.

Backend Scope
Endpoints
GET /api/builds
GET /api/builds/:id

Optional:

?system_id=sys-1
?status=failed
Example Response
{
  "builds": [
    {
      "id": "build-42",
      "system_id": "sys-1",
      "status": "success",
      "started_at": "2026-02-20T18:00:00Z",
      "duration_seconds": 124
    }
  ]
}
Requirements

Scoped to environments user has access to.

No UI-based filtering logic for authorization.

Frontend Scope
builds/
  api.rs
  models.rs
  adapter.rs
  view.rs

Adapter fallback identical to Systems view.

Acceptance Criteria

Real build history renders.

Filtering by system works.

Fallback mock data preserved.

Proper loading and error states.

Risk Level

Medium
<!-- SECTION:DESCRIPTION:END -->

## Problem Statement

Builds view currently uses static/mock build history. There is no real API backing for:
- Listing build history
- Filtering by system_id
- Filtering by status
- Build duration and timing information

---

## Goal

Implement full backend integration for build history and status.

---

## Non-Goals

- Implementing build trigger operations
- Changing build visualization UI significantly
- Adding build artifact management

---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 #1 Backend GET /api/builds endpoint implemented
- [ ] #2 #2 Backend GET /api/builds/:id endpoint implemented
- [ ] #3 #3 Query parameters for system_id and status filtering
- [ ] #4 #4 Builds scoped to environments user has access to
- [ ] #5 #5 Frontend builds/api.rs created
- [ ] #6 #6 Frontend builds/models.rs created
- [ ] #7 #7 Frontend builds/adapter.rs created with fallback logic
- [ ] #8 #8 Frontend builds/view.rs updated to use adapter
- [ ] #9 #9 Proper loading and error states implemented
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
nix develop -c cargo test --package web-ui builds
```

Manual:
- Navigate to Builds view and verify real data loads
- Test filtering by system
- Test filtering by status
- Verify fallback to mock data when backend unavailable

---

## Impact Areas

- Backend API
- Web UI

---

## Risk Level

Medium

---

## Dependencies

- TASK-121 (Systems View Backend Integration) - for system_id reference

---

## Follow-Up Tasks (if discovered during grooming)

- Add unit tests for builds adapter
- Implement build trigger operations
- Add build log viewer
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-sonnet-4-6 on gray in ~/code/crystal-forge/TASK-122-builds-backend-integration

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/134
<!-- SECTION:NOTES:END -->
