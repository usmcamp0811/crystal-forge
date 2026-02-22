---
id: TASK-79
title: 'Admin console - users, environment membership, RBAC, and audit logging'
status: Backlog
assignee: []
created_date: '2026-02-22 02:34'
labels:
  - web-ui
  - auth
  - admin
  - security
  - rbac
dependencies: []
---

## Description

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

## Acceptance Criteria
<!-- AC:BEGIN -->
### Navigation + access control
- [ ] Add an Admin-only navigation entry (e.g., “Admin” / “Server Management”).
- [ ] Non-admin users cannot access admin routes (safe denial UX).
- [ ] Role changes take effect **on next login** (documented behavior).

### Environment-scoped visibility and authorization (core security behavior)
- [ ] Systems are associated with an Environment (existing or added association as part of this task).
- [ ] A user only sees systems belonging to environments they are a member of.
- [ ] Viewer can view system details but cannot deploy/sync/perform mutating actions.
- [ ] Operator can deploy/sync/perform allowed system operations within their environments, but cannot create environments.
- [ ] Admin can perform all operations, including environment management.
- [ ] Backend enforces the above rules (UI behavior must match backend enforcement).

### Admin UI - Users (local auth mode)
- [ ] Admin Users list view shows: identifier (username/email), role, status (enabled/disabled), environments, and updated timestamp (if available).
- [ ] Admin can create a local user with:
  - [ ] required fields validated (client + server error display)
  - [ ] initial role selection (single role)
  - [ ] initial environment membership assignment (0..N)
- [ ] Admin can update a local user:
  - [ ] change role (single role)
  - [ ] change environment memberships (0..N)
  - [ ] disable/enable user (or disable at minimum)
- [ ] Guardrails:
  - [ ] Admin cannot disable/delete their own account OR cannot remove their own Admin role (choose exact guardrail and implement).
  - [ ] Destructive actions require confirmation.

### Admin UI - OIDC mapping (OIDC enabled)
- [ ] Provide an admin screen to manage mappings:
  - [ ] OIDC group → Crystal Forge role (Admin/Operator/Viewer)
  - [ ] OIDC group → Crystal Forge environments (membership)
- [ ] On login, OIDC users have role + environment memberships derived from the mapping (persisted in CF in a conventional way).
- [ ] UI clearly communicates which user attributes are IdP-derived vs locally-managed.

### Audit logging (required)
- [ ] Backend records audit events for:
  - [ ] local user create/disable/role change/env membership change
  - [ ] OIDC mapping changes
  - [ ] environment create/update/delete (if in scope)
- [ ] Admin UI includes an audit log view with:
  - [ ] filters (at minimum: event type, actor, time range)
  - [ ] empty/error/loading states

### Tests
- [ ] Unit tests exist for RBAC/environment gating logic (backend and/or UI state logic as appropriate).
- [ ] Integration/UI check coverage includes at least:
  - [ ] authenticated Admin view screenshot(s)
  - [ ] non-admin denial/redirect proof
  - [ ] environment scoping proof (e.g., two envs, user sees only one)
<!-- AC:END -->

## Verification Plan

Tier 0 (targeted):
- `nix develop -c cargo fmt -- --check`
- `nix develop -c cargo clippy -- -D warnings`
- `nix develop -c cargo test --package web-ui admin`
- `nix develop -c cargo test --package <backend-package> rbac` (use narrowest applicable selection)

Tier 1 (feature-level manual):
- Local auth mode:
  - Admin creates local user, assigns envs + role, user logs in and sees only scoped systems.
  - Viewer vs Operator behavior verified on at least one system action.
- OIDC mode:
  - Update group mappings, login as OIDC user in mapped groups, confirm role + env scoping.
- Audit log:
  - Perform admin actions, confirm audit entries appear and are filterable.

Tier 2 (recommended before MR review):
- `nix build .#checks.x86_64-linux.web-ui` (or repo-standard checks)
- If backend checks exist: `nix build .#checks.x86_64-linux.<backend-check>`

## Dependencies

- A way to associate systems with an Environment (existing model or added as part of this task).
- Backend endpoints for:
  - users + membership management (local mode)
  - group-to-role + group-to-environment mapping (OIDC mode)
  - audit event retrieval
- Auth context must expose:
  - authenticated user identity
  - role
  - environment memberships (or derived scope token)
- If sqlx applies (schema or query changes): sqlx offline metadata must be refreshed.

## Impact Areas

- Security model (RBAC + environment scoping)
- Backend authz + audit
- Web UI routing + admin views
- API DTOs + clients
- Data model (users, memberships, mappings, audit)

## Risk Level

High

Rationale:
- Security-sensitive and cross-cutting (visibility, authorization, audit).
- OIDC mapping and environment scoping can easily be misconfigured without clear UX + backend enforcement.
<!-- SECTION:DESCRIPTION:END -->
