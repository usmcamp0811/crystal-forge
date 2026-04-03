---
id: TASK-240
title: Resolve role contract for build cancel action (admin-only vs operator+)
status: Backlog
assignee: []
created_date: '2026-04-02 12:47'
labels:
  - builds
  - backend
  - authz
  - cancel-lifecycle
dependencies:
  - TASK-237
references:
  - packages/default/src/handlers/api/builders.rs
  - packages/web-ui/src/api/client.rs
  - packages/web-ui/src/views/builds.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

There is a mismatch between what the cancel action implies for users and what the server actually enforces:

- The server cancel handler (`POST /api/v1/build-jobs/{id}/cancel` in `packages/default/src/handlers/api/builders.rs`) uses `require_admin` — **admin-only**.
- The web UI `cancel_build_job` client function and associated comments in the UI code suggest operators should be able to control queue actions.
- Other queue controls (Run Next / prioritize) also use `require_admin`, so there is inconsistency across the whole set of operator-facing queue controls.

Before TASK-237's MR can be merged, this contract must be explicitly decided and implemented uniformly. Running as admin-only in production means operators (non-admin users with operator role) would get `403 Forbidden` when trying to stop a build from the Builds view, even though the UI renders the Stop button for them.

## Goal

Make a deliberate decision about the required role for cancel and other queue control actions, then implement it consistently across:
1. Server handler authorization
2. UI client call-site comments and error messages
3. Task/MR documentation

## Non-Goals

- No changes to the role model itself (admin/operator/viewer hierarchy is out of scope).
- No changes to builder-side authentication.

## Decision Required (human input needed before sprint-ready)

**Option A — Admin-only everywhere:**
- `cancel`, `prioritize`, and any future queue mutation actions require admin.
- Operators cannot stop builds from the UI even though the button is visible.
- Simplest to implement; lowest blast radius.
- UI should hide these actions from non-admin users.

**Option B — Operator+ for queue control:**
- `cancel`, `prioritize` require operator or admin.
- Operators are trusted to manage queue state.
- Requires changing `require_admin` to `require_operator_or_above` in the relevant handlers.
- More operationally useful.

## Scope (once decision is made)

### If Option A (admin-only, restrict UI):
1. Keep handler authorization as `require_admin`.
2. Update the UI to pass the user's role to the action buttons and hide/disable cancel, Run Next, Stop for non-admin users.
3. Update client comments to reflect admin-only.

### If Option B (operator+):
1. Change `require_admin` to `require_operator_or_above` in:
   - `cancel_build_job` handler
   - `prioritize_build_job` handler
2. Update client comments.
3. Verify no regression in existing RBAC tests.

## Verification Plan

### Tier 0
- `nix develop -c env SQLX_OFFLINE=true cargo check --package crystal-forge`

### Tier 1
- Log in as an operator (non-admin) user.
- Attempt to cancel a build: verify the outcome matches the chosen option (forbidden with hidden UI, or permitted).

## Impact Areas

- `packages/default/src/handlers/api/builders.rs` — cancel and prioritize handler auth
- `packages/web-ui/src/api/client.rs` — cancel_build_job comment
- `packages/web-ui/src/views/builds.rs` — action button visibility (if Option A)

## Risk Level

Low — authorization change, no schema or protocol changes.

## Dependencies

- TASK-237 MR !205 (introduces the cancel endpoint)
<!-- SECTION:DESCRIPTION:END -->
