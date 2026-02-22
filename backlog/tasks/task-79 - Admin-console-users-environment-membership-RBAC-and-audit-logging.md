---
id: TASK-79
title: 'Admin console - users, environment membership, RBAC, and audit logging'
status: To Do
assignee: []
created_date: '2026-02-22 02:34'
updated_date: '2026-02-22 02:35'
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
- [ ] #1 Add an Admin-only navigation entry (e.g., “Admin” / “Server Management”).
- [ ] #2 Non-admin users cannot access admin routes (safe denial UX).
- [ ] #3 Role changes take effect **on next login** (documented behavior).

### Environment-scoped visibility and authorization (core security behavior)
- [ ] #4 Systems are associated with an Environment (existing or added association as part of this task).
- [ ] #5 A user only sees systems belonging to environments they are a member of.
- [ ] #6 Viewer can view system details but cannot deploy/sync/perform mutating actions.
- [ ] #7 Operator can deploy/sync/perform allowed system operations within their environments, but cannot create environments.
- [ ] #8 Admin can perform all operations, including environment management.
- [ ] #9 Backend enforces the above rules (UI behavior must match backend enforcement).

### Admin UI - Users (local auth mode)
- [ ] #10 Admin Users list view shows: identifier (username/email), role, status (enabled/disabled), environments, and updated timestamp (if available).
- [ ] #11 Admin can create a local user with:
- [ ] #12 Admin can update a local user:
- [ ] #13 Guardrails:
- [ ] #14 Provide an admin screen to manage mappings:
- [ ] #15 On login, OIDC users have role + environment memberships derived from the mapping (persisted in CF in a conventional way).
- [ ] #16 UI clearly communicates which user attributes are IdP-derived vs locally-managed.
- [ ] #17 Backend records audit events for:
- [ ] #18 Admin UI includes an audit log view with:
- [ ] #19 Unit tests exist for RBAC/environment gating logic (backend and/or UI state logic as appropriate).
- [ ] #20 Integration/UI check coverage includes at least:

### Admin UI - OIDC mapping (OIDC enabled)

### Audit logging (required)

### Tests
<!-- AC:END -->
