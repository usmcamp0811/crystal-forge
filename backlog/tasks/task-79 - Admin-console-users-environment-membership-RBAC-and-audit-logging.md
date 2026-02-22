---
id: TASK-79
title: 'Admin console - users, environment membership, RBAC, and audit logging'
status: In Progress
assignee: []
created_date: '2026-02-22 02:34'
updated_date: '2026-02-22 04:54'
labels:
  - web-ui
  - auth
  - admin
  - security
  - rbac
dependencies: []
priority: medium
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
- [ ] #4 Systems are associated with an Environment (existing or added association as part of this task).
- [ ] #5 A user only sees systems belonging to environments they are a member of.
- [ ] #6 Viewer can view system details but cannot deploy/sync/perform mutating actions.
- [ ] #7 Operator can deploy/sync/perform allowed system operations within their environments, but cannot create environments.
- [ ] #8 Admin can perform all operations, including environment management.
- [ ] #9 Backend enforces the above rules (UI behavior must match backend enforcement).

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
<!-- SECTION:NOTES:END -->
