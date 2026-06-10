---
id: TASK-204
title: Fix builders view HTTP 403 Internal server error
status: To Do
assignee: []
created_date: '2026-03-20 13:40'
updated_date: '2026-06-10 03:23'
labels:
  - backend
  - frontend
  - rbac
  - bug
  - high-priority
milestone: 'm-0: Critical Bugs & Stability'
dependencies: []
references:
  - packages/default/src/handlers/api/builders.rs
  - packages/web-ui/src/views/builders.rs
modified_files:
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/auth/**
  - packages/web-ui/src/views/builders.rs
  - packages/web-ui/src/api/client.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The Builders view currently fails with an HTTP 403 / internal-server-error experience for at least one expected access path, preventing operators/admins from viewing or managing builders from the web UI.

## Goal
Identify the exact authorization or handler failure causing the Builders view to return 403/internal error responses, then restore the intended Builders page behavior for authorized users without weakening RBAC.

## Non-Goals
- No redesign of the Builders UI beyond any minimal error-state cleanup needed to verify the fix.
- No broad RBAC refactor unrelated to builders access.
- No changes to builder scheduling/concurrency behavior; those belong to TASK-291 and related reliability work.

## Scope
1. Reproduce the failing request path from the Builders UI.
2. Determine whether the failure is caused by frontend route assumptions, missing auth headers/session handling, backend RBAC policy, or handler/query errors surfaced incorrectly as 403.
3. Fix the authorized-user path so the Builders page loads and builder actions allowed by role continue working.
4. Preserve denied behavior for unauthorized users and make error semantics explicit in tests.

## Architectural Constraints
- Keep authorization decisions in the backend authorization layer/handlers, not in ad-hoc frontend conditionals.
- Preserve explicit role checks and auditability.
- Keep UI state/data mapping separate from presentation.

## Impact Areas
- `packages/default/src/handlers/api/builders.rs`
- related auth/RBAC helpers under `packages/default/src/**`
- `packages/web-ui/src/views/builders.rs`
- `packages/web-ui/src/api/client.rs`
- targeted builder integration checks/tests

## Verification Plan
Tier 1 targeted verification:
- `nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml builders`
- `nix develop -c env SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml handlers::api::builders`
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
- `nix build .#checks.x86_64-linux.web-ui`
- Manual/API verification that authorized users can load Builders view and unauthorized users still receive the correct denial.

## Risk Level
High: Builders are operational infrastructure surfaces; auth regressions could either block management or accidentally broaden access.

## Dependencies
- Existing builder CRUD and RBAC semantics remain the source of truth.
- Any broader builder runtime issues stay out of scope unless directly blocking Builders view access.
<!-- SECTION:DESCRIPTION:END -->

# Fix builders view HTTP 403 Internal server error

---

# Problem Statement

Builder view displays error: "⚠️ Failed to load builders: HTTP 403: Internal server error"

This prevents all users from viewing or managing builders in the UI, blocking operational visibility into the build infrastructure.

---

# Goal

Builders view loads successfully and displays the list of registered builders. Proper RBAC is enforced:
- **Admins**: Full read/write access (view, add, edit, delete builders)
- **Operators**: Read-only access (view builders)
- **Viewers**: Read-only access (view builders)

---

# Non-Goals

- Redesigning builders UI
- Adding new builder features
- Implementing builder registration workflow changes
- Adding builder health monitoring (separate from viewing list)

---

# Acceptance Criteria

- [ ] Builders view loads without errors for all roles
- [ ] Admin users can:
  - View list of builders
  - Add new builders (if UI supports it)
  - Edit builder configuration (if UI supports it)
  - Delete builders (if UI supports it)
- [ ] Operator and Viewer users can:
  - View list of builders (read-only)
  - Cannot modify builders (no edit/delete buttons shown)
- [ ] Backend endpoint returns appropriate data:
  - 200 OK with builder list for authorized users
  - 403 Forbidden only if user has no read access
  - 500 errors investigated and fixed
- [ ] Error handling improved:
  - Distinguish between 403 (forbidden) and 500 (server error)
  - Show appropriate user-facing message for each
  - Log server errors for debugging
- [ ] RBAC authorization correct:
  - GET /api/builders (or similar) allows all authenticated users
  - POST/PUT/DELETE /api/builders requires Admin role

---

# Architectural Constraints

- Follow existing RBAC patterns (see handlers/api/*)
- Use existing `AuthRole` enum and authorization helpers
- Builders endpoint should be in `handlers/api/` (admin or builders module)
- Builders queries in `queries/builders.rs`
- No schema changes
- Error responses follow existing API patterns
- UI error handling uses existing error display components

---

# Verification Plan

Automated:
- `nix develop -c cargo test builders::`
- `nix develop -c cargo test auth::rbac`
- `nix develop -c cargo clippy -- -D warnings`
- `nix develop -c cargo fmt -- --check`

Manual:
- Start dev stack
- Test as Admin:
  - Navigate to Builders view
  - Verify page loads without errors
  - Verify builder list displayed
  - Check browser console: no 403 or 500 errors
  - Verify admin actions visible (if applicable)
- Test as Operator:
  - Navigate to Builders view
  - Verify page loads successfully
  - Verify builder list displayed
  - Verify no edit/delete buttons (read-only)
- Test as Viewer:
  - Navigate to Builders view
  - Verify page loads successfully
  - Verify builder list displayed
  - Verify read-only access
- Check server logs:
  - No error logs on successful builder view load
  - If 500 error reproduced, logs show root cause
- Test with no builders registered:
  - Verify empty state shown (not error)

Investigation steps:
- Reproduce the 403 error
- Check server logs for stack trace
- Identify failing authorization check or query
- Verify endpoint route and RBAC decorator
- Check if endpoint exists and is properly registered

---

# Impact Areas

API | UI

- Builders API endpoint (`/api/builders` or similar)
- RBAC authorization for builders endpoints
- Builders view component (frontend)
- Error handling in both backend and frontend
- Builders queries module

---

# Risk Level

Low

This is primarily a bug fix. The endpoint should already exist but has an authorization or error handling issue. Risk is low because:
- Fixing broken functionality (not adding new features)
- RBAC patterns are well-established
- UI already exists (just needs to load data)

Risks:
- May uncover deeper authorization framework issue
- May require changes to multiple endpoints if pattern is broken

Mitigations:
- Follow existing working RBAC patterns from other endpoints
- Test all three roles thoroughly
- Review recent changes to builders endpoint or RBAC middleware

---

# Dependencies

None

---

# Follow-Up Tasks

- Add integration tests for builders RBAC
- Add builder health monitoring to builders view
- Add builder registration wizard for easier setup
- Audit all other API endpoints for similar 403/500 errors

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Authorized users can load the Builders view without HTTP 403/internal server error
- [ ] #2 Unauthorized users still receive the correct denied behavior for builders endpoints
- [ ] #3 The root cause is covered by targeted backend tests or web-ui assertions
- [ ] #4 Builders UI renders expected primary data state after the fix
- [ ] #5 No unrelated RBAC behavior is regressed for other surfaces
<!-- AC:END -->
