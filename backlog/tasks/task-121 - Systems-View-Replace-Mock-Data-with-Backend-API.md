---
id: TASK-121
title: Systems View - Replace Mock Data with Backend API
status: Done
assignee: []
created_date: '2026-02-23'
updated_date: '2026-03-13 01:24'
labels:
  - backend
  - api
  - web-ui
  - systems
milestone: m-7
dependencies: []
priority: high
ordinal: 73000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem

The Systems view currently renders mock/static data. There is no real API backing for:

Listing systems

Filtering by environment

RBAC-scoped visibility

System status

This prevents meaningful operational use and correct environment scoping.

Goal

Implement backend API endpoint(s) for Systems.

Implement DTOs and handlers.

Connect web-ui Systems view to backend.

Maintain deterministic mock fallback if backend is unavailable.

Backend Scope
Endpoints
GET /api/systems
GET /api/systems/:id

Optional query parameters:

?environment=prod
?status=healthy
Requirements

Server-side RBAC enforcement.

Server-side environment scoping.

No policy logic in UI.

Clean DTO layer.

Example Response
{
  "systems": [
    {
      "id": "sys-1",
      "name": "billing-api",
      "environment": "prod",
      "status": "healthy",
      "last_deploy": "2026-02-20T18:22:00Z"
    }
  ]
}
Frontend Scope

Introduce systems/api.rs

Introduce systems/models.rs

Introduce systems/adapter.rs

Update systems/view.rs

Adapter pattern:

Attempt API fetch

On 500/network → fallback to mock

On 401/403 → redirect to login

On empty list → render empty state

Acceptance Criteria

Systems view renders real data when DB present.

Environment scoping enforced server-side.

Fallback mock data works when backend unavailable.

No business logic in view layer.

Works in both auth modes.

Verification
nix build .#checks.x86_64-linux.default
nix build .#checks.x86_64-linux.web-ui
nix develop -c cargo test --package web-ui systems
Risk Level
<!-- SECTION:DESCRIPTION:END -->

## Problem Statement

The Systems view currently renders mock/static data. There is no real API backing for:
- Listing systems
- Filtering by environment
- RBAC-scoped visibility
- System status

This prevents meaningful operational use and correct environment scoping.

---

## Goal

1. Implement backend API endpoint(s) for Systems.
2. Implement DTOs and handlers.
3. Connect web-ui Systems view to backend.
4. Maintain deterministic mock fallback if backend is unavailable.

---

## Non-Goals

- Implementing write operations (create/update/delete systems)
- Changing the UI layout significantly
- Adding new system features beyond listing

---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 #1 #1 #1 #1 #1 Backend GET /api/systems endpoint implemented with environment filtering
- [ ] #2 #2 #2 #2 #2 #2 Backend GET /api/systems/:id endpoint implemented
- [ ] #3 #3 #3 #3 #3 #3 Server-side RBAC enforcement applied
- [ ] #4 #4 #4 #4 #4 #4 Server-side environment scoping enforced
- [ ] #5 #5 #5 #5 #5 #5 Frontend systems/api.rs created
- [ ] #6 #6 #6 #6 #6 #6 Frontend systems/models.rs created
- [ ] #7 #7 #7 #7 #7 #7 Frontend systems/adapter.rs created with fallback logic
- [ ] #8 #8 #8 #8 #8 #8 Frontend systems/view.rs updated to use adapter
- [ ] #9 #9 #9 #9 #9 #9 401/403 redirects to login
- [ ] #10 #10 #10 #10 #10 #10 500/network errors fallback to mock data
- [ ] #11 #11 #11 #11 #11 #11 Empty state renders when no data
- [ ] #12 #12 #12 #12 #12 #12 Verification commands pass

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
nix develop -c cargo test --package web-ui systems
```

Manual:
- Navigate to Systems view and verify real data loads
- Test environment filtering
- Test fallback to mock data when backend unavailable
- Verify RBAC scoping by logging in as different users

---

## Impact Areas

- Backend API
- Web UI
- RBAC enforcement

---

## Risk Level

Medium

---

## Dependencies

- Backend API foundation (auth middleware, DTO patterns)

---

## Follow-Up Tasks (if discovered during grooming)

- Add unit tests for systems adapter
- Implement write operations for systems
- Add system detail view with full information

## Notes

LOCK: claude-code on crystal-forge in ~/code/crystal-forge/TASK-121-systems-view-backend-api

IMPLEMENTATION COMPLETE — awaiting MR creation (glab not authenticated).
Branch: TASK-121-systems-view-backend-api pushed to origin.
MR URL (create manually): https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/new?merge_request%5Bsource_branch%5D=TASK-121-systems-view-backend-api&merge_request%5Btarget_branch%5D=dev
Verification: cargo test 26/26 pass, nix build .#checks.x86_64-linux.web-ui exit 0
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/133\nStatus: In Review
<!-- SECTION:NOTES:END -->

## Notes

LOCK: claude-code on crystal-forge in ~/code/crystal-forge/TASK-121-systems-view-backend-api

IMPLEMENTATION COMPLETE — awaiting MR creation (glab not authenticated).
Branch: TASK-121-systems-view-backend-api pushed to origin.
MR URL (create manually): https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/new?merge_request%5Bsource_branch%5D=TASK-121-systems-view-backend-api&merge_request%5Btarget_branch%5D=dev
Verification: cargo test 26/26 pass, nix build .#checks.x86_64-linux.web-ui exit 0
<!-- AC:END -->

## Notes

LOCK: claude-code on crystal-forge in ~/code/crystal-forge/TASK-121-systems-view-backend-api

IMPLEMENTATION COMPLETE — awaiting MR creation (glab not authenticated).
Branch: TASK-121-systems-view-backend-api pushed to origin.
MR URL (create manually): https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/new?merge_request%5Bsource_branch%5D=TASK-121-systems-view-backend-api&merge_request%5Btarget_branch%5D=dev
Verification: cargo test 26/26 pass, nix build .#checks.x86_64-linux.web-ui exit 0
<!-- AC:END -->

## Notes

LOCK: claude-code on crystal-forge in ~/code/crystal-forge/TASK-121-systems-view-backend-api

IMPLEMENTATION COMPLETE — awaiting MR creation (glab not authenticated).
Branch: TASK-121-systems-view-backend-api pushed to origin.
MR URL (create manually): https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/new?merge_request%5Bsource_branch%5D=TASK-121-systems-view-backend-api&merge_request%5Btarget_branch%5D=dev
Verification: cargo test 26/26 pass, nix build .#checks.x86_64-linux.web-ui exit 0
<!-- AC:END -->

## Notes

LOCK: claude-code on crystal-forge in ~/code/crystal-forge/TASK-121-systems-view-backend-api

IMPLEMENTATION COMPLETE — awaiting MR creation (glab not authenticated).
Branch: TASK-121-systems-view-backend-api pushed to origin.
MR URL (create manually): https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/new?merge_request%5Bsource_branch%5D=TASK-121-systems-view-backend-api&merge_request%5Btarget_branch%5D=dev
Verification: cargo test 26/26 pass, nix build .#checks.x86_64-linux.web-ui exit 0
<!-- AC:END -->

## Notes

LOCK: claude-code on crystal-forge in ~/code/crystal-forge/TASK-121-systems-view-backend-api

IMPLEMENTATION COMPLETE — awaiting MR creation (glab not authenticated).
Branch: TASK-121-systems-view-backend-api pushed to origin.
MR URL (create manually): https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/new?merge_request%5Bsource_branch%5D=TASK-121-systems-view-backend-api&merge_request%5Btarget_branch%5D=dev
Verification: cargo test 26/26 pass, nix build .#checks.x86_64-linux.web-ui exit 0
<!-- AC:END -->

## Notes

LOCK: claude-code on crystal-forge in ~/code/crystal-forge/TASK-121-systems-view-backend-api

IMPLEMENTATION COMPLETE — awaiting MR creation (glab not authenticated).
Branch: TASK-121-systems-view-backend-api pushed to origin.
MR URL (create manually): https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/new?merge_request%5Bsource_branch%5D=TASK-121-systems-view-backend-api&merge_request%5Btarget_branch%5D=dev
Verification: cargo test 26/26 pass, nix build .#checks.x86_64-linux.web-ui exit 0
<!-- AC:END -->
