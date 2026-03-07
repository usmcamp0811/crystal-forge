---
id: TASK-123
title: Deployment Policies View - Backend Integration
status: To Do
assignee: []
created_date: '2026-02-23'
updated_date: '2026-03-07 23:39'
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
## Problem

The deployment policies feature currently has a database schema and backend models (`DeploymentPolicy` enum, policy evaluation logic), but no API endpoints to expose this data to the frontend. The UI currently uses static/mock policy data, preventing users from viewing or managing real deployment policies stored in the database.

This creates a gap where:
- Backend has policies stored in `deployment_policies`, `environment_policies`, and `system_policies` tables
- Frontend cannot retrieve or display actual policy configurations
- Users cannot see which policies are enforced on their environments/systems
- RBAC rules (Admin/Operator/Viewer) cannot be applied to policy visibility and modification

## Goal

Implement two REST API endpoints to expose deployment policies from the backend database, and integrate these endpoints into the frontend using role-based access control. This enables:

1. **Backend**: Expose deployment policy data through GET endpoints with RBAC enforcement
2. **Frontend**: Replace mock policy data with live API calls, respecting user roles
3. **Auth**: Only Admin/Operator users see modification actions; Viewer users have read-only access

This establishes the foundational API for future policy CRUD operations (POST, PUT, DELETE) while maintaining backward compatibility through fallback logic.

## Non-Goals

- Policy creation/modification/deletion endpoints (POST/PUT/DELETE) - future work
- Policy evaluation or validation logic - already exists in backend
- Migration of existing deployment_policies schema - already complete
- Complex policy filtering or search - simple list/detail retrieval only
- WebSocket or real-time policy updates
- Policy templates or wizards
- Bulk policy operations
- Policy versioning or audit history (beyond existing timestamps)
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
- [ ] #1 Backend GET /api/v1/deployment-policies endpoint returns all policies with pagination support (limit/offset query params)
- [ ] #2 Backend GET /api/v1/deployment-policies/:id endpoint returns single policy details or 404
- [ ] #3 Server-side RBAC: All authenticated users (Admin/Operator/Viewer) can read policies
- [ ] #4 Server-side RBAC: Endpoint responses include user role in a way that frontend can consume
- [ ] #5 Frontend packages/web-ui/src/api/deployment_policies.rs module created with fetch_policies() and fetch_policy_by_id() functions
- [ ] #6 Frontend packages/web-ui/src/models/deployment_policy.rs defines DeploymentPolicyDTO matching backend API response
- [ ] #7 Frontend adapter layer (packages/web-ui/src/components/policies/adapter.rs) implements fetch-with-fallback: try API first, fall back to mock on error
- [ ] #8 Frontend policies view (packages/web-ui/src/components/policies/view.rs) uses adapter instead of direct mock data
- [ ] #9 Role-based UI behavior: Admin/Operator users see 'Edit' and 'Delete' buttons (disabled/hidden for future work)
- [ ] #10 Role-based UI behavior: Viewer users see policies but no modification actions
- [ ] #11 401/403 responses redirect to login page using existing auth error handler
- [ ] #12 500/network errors fall back to mock data without breaking the UI
- [ ] #13 Backend unit tests verify RBAC enforcement for policy endpoints

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

- [ ] #14 Frontend compiles without errors after integration
- [ ] #15 cargo fmt --check passes for modified Rust files
- [ ] #16 cargo clippy -- -D warnings passes for backend changes
- [ ] #17 nix build .#server succeeds (integration check)
- [ ] #18 nix build .#web-ui succeeds (integration check)
<!-- AC:END -->
