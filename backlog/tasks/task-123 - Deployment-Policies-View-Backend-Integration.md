---
id: TASK-123
title: Deployment Policies View - Backend Integration
status: To Do
assignee: []
created_date: '2026-02-23'
updated_date: '2026-03-07 23:45'
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

The deployment policies feature currently has a database schema and backend models (`DeploymentPolicy` enum, policy evaluation logic), but no API endpoints to expose or manage this data. The UI currently uses static/mock policy data, preventing users from:

- Viewing actual deployment policies stored in the database
- Creating new deployment policies through the UI
- Modifying existing policy configurations
- Deleting obsolete or incorrect policies
- Applying RBAC rules to policy management (Admin/Operator can modify, Viewer is read-only)

This creates a critical gap where the deployment policies infrastructure exists but cannot be managed through the application interface.

## Goal

Implement a complete REST API for deployment policy management (full CRUD operations) and integrate it into the frontend with role-based access control. This enables:

1. **Backend**: Full CRUD endpoints (GET, POST, PUT, DELETE) for deployment policies with RBAC enforcement
2. **Frontend**: Complete policy management UI that replaces mock data with live API calls
3. **Auth**: Admin/Operator users can create/edit/delete policies; Viewer users have read-only access
4. **UX**: Graceful error handling with fallback to mock data for read operations when server is unavailable

Users will be able to fully manage deployment policies through the web interface, with appropriate permissions enforced at both API and UI layers.

## Non-Goals

- Policy evaluation or validation logic - already exists in backend
- Migration of existing deployment_policies schema - already complete (migration 0080)
- Policy assignment to environments/systems - separate feature (uses environment_policies/system_policies tables)
- Complex policy filtering or advanced search - simple list/detail retrieval only
- WebSocket or real-time policy updates
- Policy templates or creation wizards (basic form-based creation is sufficient)
- Bulk policy operations (import/export)
- Policy versioning or detailed audit history (beyond existing timestamps)
- Policy scheduling or conditional activation
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
- [ ] #1 Backend GET /api/v1/deployment-policies endpoint returns all policies with pagination (limit/offset)
- [ ] #2 Backend GET /api/v1/deployment-policies/:id endpoint returns single policy or 404
- [ ] #3 Backend POST /api/v1/deployment-policies endpoint creates new policy (Admin/Operator only)
- [ ] #4 Backend PUT /api/v1/deployment-policies/:id endpoint updates existing policy (Admin/Operator only)
- [ ] #5 Backend DELETE /api/v1/deployment-policies/:id endpoint deletes policy (Admin only)
- [ ] #6 Server-side RBAC: GET endpoints allow all authenticated users (Admin/Operator/Viewer)
- [ ] #7 Server-side RBAC: POST/PUT endpoints require Admin or Operator role (403 otherwise)
- [ ] #8 Server-side RBAC: DELETE endpoint requires Admin role only (403 otherwise)
- [ ] #9 Input validation: Policy name required, max 255 chars
- [ ] #10 Input validation: Policy config must be valid JSON matching policy_type schema
- [ ] #11 Input validation: policy_type must be one of: require_cf_agent, require_packages, custom_check
- [ ] #12 Duplicate name prevention: POST/PUT returns 409 if policy name already exists
- [ ] #13 Referential integrity: DELETE returns 409 if policy is assigned to environments/systems

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

- [ ] #14 Frontend API client (packages/web-ui/src/api/deployment_policies.rs) implements all 5 CRUD operations
- [ ] #15 Frontend models (packages/web-ui/src/models/deployment_policy.rs) include CreatePolicyRequest, UpdatePolicyRequest DTOs
- [ ] #16 Frontend adapter layer (packages/web-ui/src/components/policies/adapter.rs) implements fetch-with-fallback for read ops
- [ ] #17 Frontend policies view (packages/web-ui/src/components/policies/view.rs) uses adapter for listing
- [ ] #18 Frontend Create Policy modal with form (name, description, type selector, config JSON editor)
- [ ] #19 Frontend Edit Policy modal pre-populated with current values
- [ ] #20 Frontend Delete confirmation dialog with warning about environment/system assignments
- [ ] #21 Role-based UI: Admin/Operator see Create/Edit/Delete buttons enabled
- [ ] #22 Role-based UI: Viewer sees policies but all modification buttons hidden
- [ ] #23 401/403 responses redirect to login page
- [ ] #24 500/network errors on read operations fall back to mock data
- [ ] #25 400/409 validation errors display helpful messages in UI
- [ ] #26 Backend unit tests for all 5 endpoints covering RBAC, validation, error cases
- [ ] #27 Frontend compiles without errors
- [ ] #28 cargo fmt --check passes
- [ ] #29 cargo clippy -- -D warnings passes
- [ ] #30 nix build .#server succeeds
- [ ] #31 nix build .#web-ui succeeds
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

## NEW IMPLEMENTATION PLAN (FULL CRUD)
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
- [ ] #1 All 31 acceptance criteria checked and verified
- [ ] #2 Backend: All 5 CRUD endpoints return correct responses (tested with curl/Postman)
- [ ] #3 Backend: RBAC enforcement verified for all endpoints (Admin/Operator/Viewer)
- [ ] #4 Backend: Input validation works (name length, config JSON, policy_type enum)
- [ ] #5 Backend: Duplicate name prevention returns 409 (tested)
- [ ] #6 Backend: Referential integrity check prevents deleting in-use policies (409)
- [ ] #7 Frontend: Policies list displays data from backend API
- [ ] #8 Frontend: Create Policy modal creates policies successfully
- [ ] #9 Frontend: Edit Policy modal updates policies successfully
- [ ] #10 Frontend: Delete confirmation deletes policies successfully
- [ ] #11 Frontend: Role-based UI shows/hides buttons correctly (Admin/Operator/Viewer tested)
- [ ] #12 Frontend: 401/403 redirects to login (tested by removing session)
- [ ] #13 Frontend: 500/network errors fall back gracefully (tested by stopping server)
- [ ] #14 Frontend: 400/409 validation errors display helpful messages in UI
- [ ] #15 Frontend: No console errors or warnings in browser dev tools
- [ ] #16 cargo fmt --check passes for all modified Rust files
- [ ] #17 cargo clippy -- -D warnings passes for backend package
- [ ] #18 nix build .#server succeeds
- [ ] #19 nix build .#web-ui succeeds
- [ ] #20 All new Rust files tracked in Git (git status clean)
- [ ] #21 Code follows repository patterns (mirrors builders.rs API style)
- [ ] #22 No unwrap() calls in production code paths
- [ ] #23 Backend unit tests exist for all 5 endpoints
- [ ] #24 Backend unit tests cover RBAC variations
- [ ] #25 Backend unit tests cover validation and error cases
- [ ] #26 All backend unit tests pass (cargo test)
- [ ] #27 Manual E2E test: Admin creates policy via UI → success
- [ ] #28 Manual E2E test: Operator edits policy → success
- [ ] #29 Manual E2E test: Viewer cannot create/edit/delete (buttons hidden/disabled)
- [ ] #30 Manual E2E test: Admin deletes unused policy → success
- [ ] #31 Manual E2E test: Delete in-use policy → 409 error with clear message
<!-- DOD:END -->
