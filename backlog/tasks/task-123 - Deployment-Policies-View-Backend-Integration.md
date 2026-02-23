---
id: TASK-123
title: Deployment Policies View - Backend Integration
status: To Do
assignee: []
created_date: '2026-02-23'
updated_date: '2026-02-23 21:18'
labels:
  - backend
  - api
  - web-ui
  - policies
milestone: m-13
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Deployment Policies View: Backend Integration
Problem

Policies are currently static in UI.

Goal

Expose deployment policies from backend and render dynamically.

Backend Scope
Endpoints
GET /api/deployment-policies
GET /api/deployment-policies/:id

Future-compatible for:

POST /api/deployment-policies
PUT /api/deployment-policies/:id
Example Response
{
  "policies": [
    {
      "id": "policy-1",
      "name": "prod-approval",
      "environment": "prod",
      "requires_approval": true,
      "min_approvers": 2
    }
  ]
}
Requirements

RBAC enforcement server-side.

Only Admin/Operator see modify actions.

Viewer is read-only.

Frontend Scope

Policy DTOs.

Role-aware action visibility driven by auth context.

Fallback to mock data.

Acceptance Criteria

Policies render dynamically.

Role-based UI behavior correct.

Fallback logic intact.

Risk Level

Medium
<!-- SECTION:DESCRIPTION:END -->

## Problem Statement

Policies are currently static in UI. There is no real API backing for:
- Listing deployment policies
- Viewing policy details
- RBAC-aware action visibility

---

## Goal

Expose deployment policies from backend and render dynamically with proper RBAC.

---

## Non-Goals

- Implementing policy create/update operations (future-compatible only)
- Changing policy visualization UI significantly

---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 #1 Backend GET /api/deployment-policies endpoint implemented
- [ ] #2 #2 Backend GET /api/deployment-policies/:id endpoint implemented
- [ ] #3 #3 Server-side RBAC enforcement applied
- [ ] #4 #4 Only Admin/Operator see modify actions
- [ ] #5 #5 Viewer role is read-only
- [ ] #6 #6 Frontend policies/api.rs created
- [ ] #7 #7 Frontend policies/models.rs created
- [ ] #8 #8 Frontend policies/adapter.rs created with fallback logic
- [ ] #9 #9 Frontend policies/view.rs updated to use adapter
- [ ] #10 #10 Role-based action visibility driven by auth context
- [ ] #11 #11 401/403 redirects to login
- [ ] #12 #12 500/network errors fallback to mock data
- [ ] #13 #13 Verification commands pass

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
nix develop -c cargo test --package web-ui policies
```

Manual:
- Navigate to Policies view and verify real data loads
- Test as Admin/Operator - verify modify actions visible
- Test as Viewer - verify modify actions hidden
- Verify fallback to mock data when backend unavailable

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

- TASK-121 (Systems View Backend Integration)

---

## Follow-Up Tasks (if discovered during grooming)

- Add unit tests for policies adapter
- Implement policy create/update operations
- Add policy validation UI
<!-- AC:END -->
