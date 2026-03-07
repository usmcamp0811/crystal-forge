---
id: TASK-123
title: Deployment Policies View - Backend Integration
status: To Do
assignee: []
created_date: '2026-02-23'
updated_date: '2026-03-07 23:40'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation Plan

### Phase 1: Backend API Endpoints (packages/default)

1. **Create deployment policies queries module** (`packages/default/src/queries/deployment_policies.rs`)
   - `list_deployment_policies(pool, limit, offset)` - Returns Vec<DeploymentPolicy>
   - `get_deployment_policy_by_id(pool, id)` - Returns Option<DeploymentPolicy>
   - Use existing `deployment_policies` table schema from migration 0080

2. **Create API handlers module** (`packages/default/src/handlers/api/deployment_policies.rs`)
   - `list_deployment_policies()` handler
     - Extract user with `RequireAuth` (any authenticated role)
     - Parse query params: `limit` (default 100, max 1000), `offset` (default 0)
     - Call query layer
     - Return JSON response with policies array
   - `get_deployment_policy()` handler
     - Extract user with `RequireAuth`
     - Parse UUID from path param
     - Call query layer
     - Return 404 if not found, 200 with JSON if found

3. **Register routes in router** (`packages/default/src/handlers/api/mod.rs`)
   - Add `mod deployment_policies;`
   - Register routes under `/api/v1/deployment-policies`

4. **Add API response models** (if not already in `packages/default/src/api/models.rs`)
   - Verify `DeploymentPolicySummary` exists and matches DB schema
   - Add `DeploymentPoliciesListResponse { policies: Vec<DeploymentPolicySummary> }`

### Phase 2: Frontend Integration (packages/web-ui)

5. **Create API client module** (`packages/web-ui/src/api/deployment_policies.rs`)
   - `fetch_policies(limit, offset) -> Result<Vec<DeploymentPolicyDTO>, ApiError>`
   - `fetch_policy_by_id(id) -> Result<DeploymentPolicyDTO, ApiError>`
   - Use existing HTTP client patterns from other API modules (e.g., `api/flakes.rs`)

6. **Define frontend models** (`packages/web-ui/src/models/deployment_policy.rs` or update existing)
   - Verify `DeploymentPolicyDTO` matches backend response
   - Add `#[derive(Clone, PartialEq, serde::Deserialize)]`

7. **Create adapter layer** (`packages/web-ui/src/components/policies/adapter.rs`)
   - `load_policies_with_fallback() -> Vec<DeploymentPolicyDTO>`
   - Try `fetch_policies()`, on error log warning and return mock data
   - Handle 401/403 by redirecting to login (use existing auth helpers)
   - Handle 5xx/network errors with silent fallback

8. **Update policies view** (`packages/web-ui/src/components/policies/view.rs`)
   - Replace direct mock data usage with `load_policies_with_fallback()`
   - Add conditional rendering for Edit/Delete buttons based on user role
   - Extract role from auth context (check existing auth state management)

### Phase 3: Testing & Verification

9. **Backend unit tests** (`packages/default/src/handlers/api/deployment_policies.rs` inline or separate test module)
   - Test list endpoint with various auth roles
   - Test detail endpoint with valid/invalid IDs
   - Test pagination parameters

10. **Integration verification**
    - Run `cargo fmt --check`
    - Run `cargo clippy -- -D warnings`
    - Build server: `nix build .#server`
    - Build web-ui: `nix build .#web-ui`
    - Manual test: Start `server-stack up` and verify UI loads policies
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Architectural Constraints

- **Use Axum extractors for RBAC**: Follow modern pattern with `RequireAuth`, not legacy function guards
- **No business logic in handlers**: Handlers orchestrate, queries execute DB operations
- **Module size limit**: Keep handlers under 500 lines; split if needed
- **Error handling**: Use `Result<T, (StatusCode, String)>` pattern from existing handlers
- **Pagination**: Default limit 100, max 1000 (prevent DoS)
- **DTOs match exactly**: Frontend models must match backend API responses (field names, types)
- **Fallback is silent**: Log errors but don't alert users on fallback to mock data
- **No unwrap() in handlers**: Use proper error propagation
- **Follow existing patterns**: Mirror structure from `handlers/api/builders.rs` and `api/flakes.rs`

## Dependencies

- ✅ TASK-65.1: Identity and RBAC data model (Done) - Required for `RequireAuth` extractor
- ✅ Migration 0080: `deployment_policies` table schema (Done) - Required for DB queries
- ✅ Backend models: `DeploymentPolicy` enum exists in `models/deployment_policies.rs`
- Frontend auth context: Must have user role available (assumed present from TASK-65.0)

## Impact Areas

- **Backend**: New API module, new queries module, router registration
- **Frontend**: New API client, adapter layer, view component updates
- **Database**: Read-only queries, no schema changes
- **Auth**: Uses existing RBAC extractors, no new auth logic
- **Tests**: New unit tests for API endpoints

## Related Tasks

- Future: POST/PUT/DELETE endpoints for policy CRUD (create follow-up task)
- Future: Policy assignment to environments/systems (separate feature)
- Future: Policy evaluation preview in UI
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 All 18 acceptance criteria are checked and verified
- [ ] #2 Backend GET endpoints return expected JSON responses (verified with curl or similar)
- [ ] #3 Frontend UI displays policies from backend API (verified in browser with dev tools)
- [ ] #4 Role-based button visibility works correctly for Admin/Operator/Viewer (manually tested)
- [ ] #5 Fallback to mock data occurs gracefully on API errors (tested by stopping server)
- [ ] #6 No console errors or warnings in browser dev tools
- [ ] #7 cargo fmt --check passes for all modified Rust files
- [ ] #8 cargo clippy -- -D warnings passes for backend package
- [ ] #9 nix build .#server completes successfully
- [ ] #10 nix build .#web-ui completes successfully
- [ ] #11 All new Rust files are tracked in Git (git status shows no untracked source files)
- [ ] #12 Code follows existing repository patterns (handlers match builders.rs style, API client matches flakes.rs style)
- [ ] #13 No unwrap() calls in production code paths
- [ ] #14 Error responses include helpful messages (tested with invalid IDs, missing auth)
- [ ] #15 Unit tests exist for both API endpoints and pass
- [ ] #16 Manual smoke test: server-stack up, navigate to policies page, verify data loads
<!-- DOD:END -->
