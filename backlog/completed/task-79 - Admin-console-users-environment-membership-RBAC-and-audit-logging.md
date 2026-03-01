---
id: TASK-79
title: 'Admin console - users, environment membership, RBAC, and audit logging'
status: Done
assignee: []
created_date: '2026-02-22 02:34'
updated_date: '2026-02-23 03:15'
labels:
  - web-ui
  - auth
  - admin
  - security
  - rbac
dependencies: []
priority: medium
ordinal: 68000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

Crystal Forge supports authentication (local + OIDC) and role-aware UI behavior, but administrators lack an in-product way to:

- manage users (local users)
- map OIDC identities/groups into Crystal Forge authorization
- assign users to environments
- enforce environment-scoped visibility for systems
- audit administrative/security-sensitive actions

This forces manual workflows and creates risk of misconfiguration, inconsistent access control, and poor traceability.

## Goal

Deliver an **Admin Server Management** area (separate from fleet management) that enables:

1. **Environment-scoped RBAC**
   - Users may belong to multiple environments.
   - Users can only see systems belonging to environments they are a member of.
   - Role determines allowed actions within those environments:
     - Viewer: read-only (cannot deploy/sync/etc.)
     - Operator: can deploy/sync/manage systems, but cannot create environments
     - Admin: full access (including environment administration)

2. **User and membership management**
   - Local auth mode: Admin can create/disable users, assign role, assign environment memberships.
   - OIDC mode: rely on conventional OIDC group mapping for role and environment membership (admin-configurable mapping in Crystal Forge).

3. **Audit logging**
   - Record admin/security-sensitive actions (who/what/when/where) for user, environment, membership, role, and configuration changes.

## Non-Goals

- Full IdP administration (creating users/groups in the IdP, MFA, OIDC client configuration UI, etc.)
- UI-only authorization (backend must enforce RBAC)
- Fine-grained policy authoring beyond the fixed roles (Admin/Operator/Viewer)
- Multi-tenant org/team management beyond “environment membership”
- Bulk import/export of users or environments
- Complex workflow automation (invitations, email delivery, SCIM provisioning) unless explicitly listed in AC

## Scope Boundaries (explicit)

**In scope (this task):**
- Admin UI screens + API integration for:
  - listing users and their role + env memberships
  - managing local users (create/disable, assign role, assign env memberships)
  - defining OIDC mappings (group → role and group → environments) used at login
  - viewing audit log entries
- Backend enforcement required for:
  - environment scoping of system visibility
  - role-based action authorization
  - audit event capture

**Out of scope unless added to AC:**
- Editing OIDC provider settings/metadata beyond mapping
- User password reset flows beyond “disable user” (local auth)
- System-level permissions beyond environment + role
- UI for advanced auditing analytics/export

## Architectural Constraints

- UI composes reusable components; no policy logic duplication in views.
- Role checks consume backend-provided auth context.
- UI must not import infrastructure layer directly.
- DTOs mirror server models; keep API models separate from UI view state.
- No unwrap in production paths; explicit errors + safe UX.
- Authorization is enforced server-side; UI reflects backend auth context.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
### Navigation + access control
- [x] #1 Add an Admin-only navigation entry (e.g., “Admin” / “Server Management”).
- [x] #2 Non-admin users cannot access admin routes (safe denial UX).
- [x] #3 Role changes take effect **on next login** (documented behavior).

### Environment-scoped visibility and authorization (core security behavior)
- [x] #4 Systems are associated with an Environment (existing or added association as part of this task).
- [x] #5 A user only sees systems belonging to environments they are a member of.
- [x] #6 Viewer can view system details but cannot deploy/sync/perform mutating actions.
- [x] #7 Operator can deploy/sync/perform allowed system operations within their environments, but cannot create environments.
- [x] #8 Admin can perform all operations, including environment management.
- [x] #9 Backend enforces the above rules (UI behavior must match backend enforcement).

### Admin UI - Users (local auth mode)
- [x] #10 Admin Users list view shows: identifier (username/email), role, status (enabled/disabled), environments, and updated timestamp (if available).
- [x] #11 Admin can create a local user with email, optional display name, initial role, and initial environment memberships; API returns validation errors for invalid email/duplicate email.
- [x] #12 Admin can update a local user role, enabled/disabled status, and environment memberships; role/membership changes are persisted and reflected in subsequent auth context after next login.
- [x] #13 Guardrails: cannot disable the last enabled admin, cannot remove the final admin role assignment, and non-admin callers receive `403` for all admin mutation endpoints.

### Admin UI - OIDC mapping (OIDC enabled)
- [x] #14 Provide an admin screen to manage mappings for `group -> role` and `group -> environments`, including create/edit/delete, duplicate detection, and input validation.
- [x] #15 On login, OIDC users have role + environment memberships derived from the mapping (persisted in CF in a conventional way).
- [x] #16 UI clearly communicates which user attributes are IdP-derived vs locally-managed.

### Audit logging (required)
- [x] #17 Backend records audit events for user create/update/disable, role changes, environment membership changes, and OIDC mapping changes, with actor, target, action, timestamp, and request origin metadata.
- [x] #18 Admin UI includes an audit log view with timestamp, actor, action, target, and filter controls (actor, action type, date range), plus pagination.

### Tests
- [x] #19 Unit tests exist for RBAC/environment gating logic (backend and/or UI state logic as appropriate).
- [x] #20 Integration/UI check coverage includes at least: non-admin route denial, admin users list render, role-based mutation denial (viewer/operator), and environment-scoped systems visibility.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode-gpt-5.3-codex on gray in /home/mcamp/code/crystal-forge/TASK-79-admin-console-rbac-audit

Progress update:
- Added admin navigation and route denial UX in web UI (admin-only sidebar entry, guarded admin route rendering).
- Added admin users list API + UI wiring with role/status/environments/updated columns.
- Added admin audit log API + UI with actor/action/date filters and pagination.
- Added backend tests for audit filter/pagination helpers and role precedence.
- Added web UI auth helper unit tests for admin/operator role gating.

Remaining major scope:
- Environment membership persistence/enforcement across systems visibility/actions.
- OIDC mapping CRUD and IdP-derived/local attribute UX distinctions.
- Expanded audit event capture for required admin mutation actions (OIDC mapping events still pending).
- Integration/UI coverage for AC #20 scenarios.

Latest progress:
- Implemented admin mutation endpoints: create user and update user.
- Added guardrails preventing disabling/removing the final enabled admin.
- Added user environment memberships persistence table and API wiring.
- Added admin UI create/edit controls for role/status/environment assignments.
- Added guardrail predicate tests in admin handler module.
- Added dedicated admin audit event storage and capture for user create/update, role, status, and environment membership changes.
- Documented next-login role/environment propagation in Admin Users UI copy.
- Added OIDC mapping validation for normalized group names, duplicate environment detection, and unknown environment rejection.
- Added explicit identity source markers in Admin Users and disabled direct edits for IdP-derived users.
- Added dedicated OIDC mapping derivation unit coverage for role precedence and environment normalization at login.
- Added audit metadata coverage for request-origin extraction precedence and persisted admin audit action keys.
- Added AppShell route-guard unit coverage for non-admin denial behavior on the admin route.
- Added admin-role predicate unit coverage that explicitly denies operator/viewer-only role sets.
- Added admin view helper tests covering user-row draft shaping and environment display defaults.
- Added systems-list environment filter tests for case-insensitive membership visibility behavior.
- Added admin users render-state helper coverage for loading/error/table view transitions.
- Completed AC #20 minimum coverage set across AppShell/admin/systems/admin-guard test paths.
- Added shared role-capability policy helpers with unit coverage for viewer/operator/admin permission matrix.
- Added environment-scope access helper coverage for role-aware membership checks.
- Added web UI auth capability helpers/tests for mutate-systems vs manage-environments role gating.
- Enforced operator-or-admin authorization on flake mutation APIs (create/delete) with explicit forbidden-path tests.
- Added shared backend API RBAC guard module and wired admin/flake handlers to common session-role checks.
- Enforced viewer-or-above session gating on dashboard summary and flake list read APIs with forbidden-path tests.
- Added backend systems list/detail APIs with membership-scoped visibility filtering and viewer-or-above authentication gating.
- Added systems endpoint tests for authenticated-role requirements and filter helper behavior.
- Added backend sync/rollback system mutation endpoints with operator-or-admin checks and environment-scope enforcement.
- Updated system detail UI to call sync/rollback APIs and disable mutation actions for viewer-role users.
- Refactored admin API data access into `queries/admin.rs` and removed inline SQL from `handlers/api/admin.rs` to align with query-layer conventions.
- Removed inline SQL from `handlers/api/auth_oidc.rs`, `handlers/api/systems.rs`, `handlers/api/auth_local.rs`, and `handlers/api/auth_status.rs` by moving DB calls into query modules.
- Added audit events for system sync/rollback mutation routes and surfaced new audit action labels in Admin UI filters/table.
- Hardened last-admin guardrails with transaction + `FOR UPDATE` locking for disable/demotion race safety.
- Updated OIDC login membership sync to preserve existing memberships when mappings are empty or unresolved, preventing accidental lockout on mapping/claim drift.
- Enforced disabled-user auth lockout by checking `users.is_active` in shared RBAC session resolution and local-auth login flow.
- Tightened system rollback input validation (length + hexadecimal format) with explicit `400` responses on invalid targets.
- Updated environment-scope access semantics so systems without an environment are admin-only.
- Created follow-up TASK-116 for admin/audit/systems query-path performance optimization.
- Added client-side validation for admin environment assignment inputs and explicit UX guidance that wildcard patterns are not currently supported.
- Added initial-password support to admin user creation (UI + API) with minimum-length validation.
- Created follow-up TASK-117 for secure password reset/recovery flow design and implementation.
- Added admin user deletion endpoint + UI action with final-admin guardrail protection and audit event capture.
- Added users-list filtering controls (search + enabled/disabled status) in Server Management.
- MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/129
<!-- SECTION:NOTES:END -->
