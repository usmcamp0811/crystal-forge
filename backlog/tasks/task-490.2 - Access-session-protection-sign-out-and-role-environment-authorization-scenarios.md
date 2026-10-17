---
id: TASK-490.2
title: >-
  Access: session protection, sign-out, and role/environment authorization
  scenarios
status: Backlog
assignee: []
created_date: '2026-10-17 16:34'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.1
references:
  - docs/knowledge/testing/integration-test-contract.md
  - docs/knowledge/testing/integration-coverage-map.md
  - docs/knowledge/security/session-cookies-and-csrf.md
  - docs/knowledge/security/api-authentication-and-authorization.md
  - tests/integration/access/scenarios/lifecycle-local-bootstrap.nix
parent_task_id: TASK-490
priority: high
type: task
ordinal: 514000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-access. Contract IDs: access.lifecycle.sessions, access.lifecycle.authorization, access.lifecycle.registration-closed (see docs/knowledge/testing/integration-coverage-map.md).

WHY: session protection and role restrictions are security boundaries. Today they are proven only by mocked browser steps (13i, 15i, 14d) and isolated Rust tests; no integration scenario proves that a denied browser/API action has no server-side effect.

OBJECTIVE: two scenario files under tests/integration/access/scenarios/: `lifecycle-session-protection` and `lifecycle-role-restrictions`. Test-only. Use the foundation harness exactly as committed.

NON-GOALS: OIDC (TASK-490.6); form persistence (TASK-490.3); shell/navigation (TASK-490.4); session TTL expiry timing; any production code change. Do not edit tests/integration/lib/**, tests/integration/access/default.nix, flake.nix, CI, or docs/design/**.

BASE: start your worktree from the foundation commit recorded in the implementation notes (FOUNDATION_COMMIT). Prerequisite already integrated there: harness + access/scenarios/lifecycle-local-bootstrap (use its .nix/.py/.cjs as the pattern).

AUTHORITY (read these; summary is not evidence): docs/knowledge/security/session-cookies-and-csrf.md (Session Cookie, Session Lifecycle, CSRF Strategy); docs/knowledge/security/api-authentication-and-authorization.md; docs/knowledge/ui/admin-console-and-login-views.md. These are as-built docs: mark scenario headers `Authority: as-built (D1)` per the coverage map.

PRODUCTION SOURCE TO READ (read-only): packages/default/crates/cf-server/src/auth/session.rs (cookie names __Host-cf-session, __Host-cf-csrf, CSRF header x-csrf-token); src/handlers/api/auth_session.rs (logout, validate_csrf, RequireCsrf); src/handlers/api/user_sessions.rs (list_sessions, revoke_session, revoke_all_sessions; routes /api/v1/user/sessions*); src/handlers/api/rbac.rs (require_admin, require_operator_or_admin, require_viewer_or_above); src/handlers/api/admin.rs (create_user + CreateAdminUserRequest {email, display_name, password, role: Admin|Operator|Viewer, environments}; route POST /api/v1/admin/users); src/handlers/api/auth_local.rs (register; note register never reads allow_registration); src/handlers/api/auth_status.rs; src/bin/server.rs:1233-1275 (auth routes); migrations/0071_auth_identity_rbac.sql (users, user_role_assignments, user_sessions); packages/web-ui/src/views/login.rs, register.rs, components/layout/sidebar.rs.

WRITABLE (exclusive): tests/integration/access/scenarios/lifecycle-session-protection.{nix,py,cjs}, tests/integration/access/scenarios/lifecycle-role-restrictions.{nix,py,cjs}. READ-ONLY: everything else, including the harness, other scenarios, production code, docs/design/CrystalForge/.

HARNESS (exact signatures in docs/knowledge/testing/integration-coverage-map.md#harness-interface): nodes.controller = harness.roles.controller { browser = true; }; Python: case, expect, expect_equal, receipt, receipts, wait_for, http(node, method, path, body=None, session=None, csrf=False), sql_json(node, query), run_browser(node, spec); browser ctx: page, expect, facts, receipts, emit, url, step, screenshot, whoami. Setup of the first admin: reuse the API path (POST /api/auth/local/register then POST /api/auth/local/login with a named http() session) as setup; the browser bootstrap flow is already proven by lifecycle-local-bootstrap and is not repeated. Create Operator and Viewer users as setup through POST /api/v1/admin/users with the admin session (csrf=True). Facts: users from harness.facts.localUserFromDesign with design ids u1 (admin), u2 (operator), u4 or another viewer id whose design role is viewer (verify with harness.facts.byId "admin.users" <id>; key by id). Environment names: authored, e.g. "staging" and "production" (design environments are keyed by name).

SCENARIO lifecycle-session-protection (actor: signed-in admin):
1. Mutation with session cookie but no x-csrf-token -> 403 csrf_validation_failed; the protected row (pick one admin mutation, e.g. PUT /api/v1/admin/classification-config) is unchanged (read via sql_json before/after).
2. Mismatched x-csrf-token -> 403, no change. Correct token -> success and the change persists.
3. Two sessions for one user (two http sessions); DELETE /api/v1/user/sessions/:id revokes the other; its next request is unauthenticated (401/whoami false) and user_sessions.invalidated_at is set.
4. Browser: sign in, sign out through the UI control; whoami false after reload; session row invalidated; protected route redirects to /login.
Prohibited side effects: no row change on rejected requests; no new session on failed auth.

SCENARIO lifecycle-role-restrictions (actors: Operator and Viewer created by admin, environment-scoped):
1. Viewer and Operator receive 403 for admin mutations (create user, PATCH user, classification config) and the protected rows are unchanged.
2. Environment scope: a user scoped to one environment must not read or mutate a resource in another environment where the API documents scoping (read admin.rs/environments.rs and docs to find the exact endpoint; if scoping for local users is not documented as accepted, record the gap in task notes instead of inventing an expectation).
3. Browser as Viewer: admin-only navigation/actions show the approved unavailable state (or are absent) and a direct /admin visit does not expose mutation controls; assert no mutation request succeeds.
4. Registration closed: with default allow_registration=false and one existing user, POST /api/auth/local/register for a new user must not create a user (expected per setup-status.allow_registration semantics and RegisterView redirect). If it does, keep the failing assertion and follow PRODUCT DEFECT below.
UI states: denied/unavailable action, return navigation to dashboard.

COMMANDS (syntax verified on the foundation slice): nix build .#integration.access.scenarios.lifecycle-session-protection -L ; nix build .#integration.access.scenarios.lifecycle-role-restrictions -L ; evidence after failure: append .run to the attribute. Run at most two VM tests at once on this host. Evidence paths: result -> evidence/result.json, evidence/browser/<spec>/, evidence/journals/controller/. The coordinator runs .#integration.access.all.

COMPLETION: both scenarios discovered (nix eval .#integration.access.scenarios --apply builtins.attrNames lists them) and executed; every assertion derives expectations from facts or the cited authority, not from CF responses; each passes, or fails only on a recorded product defect with a linked Backlog task; task notes contain exact commands, revision, outcome, evidence store paths; commit only the writable files on your branch.

PRODUCT DEFECT: keep the correct failing assertion; search backlog for a match; else create a Backlog bug (contract ID, app SHA, minimal steps, expected/actual, evidence path) and link it here. Never fix product code, weaken, skip, or retry.

ESCALATION: shared-interface gaps (missing helper, role, or fixture semantics) or authority conflicts go to the coordinator (TASK-490 owner, claude-opus-5-5) via a task note; do not edit coordinator-owned files.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 lifecycle-session-protection and lifecycle-role-restrictions exist only under tests/integration/access/scenarios/ and are discovered by mkOwner
- [ ] #2 Missing and mismatched CSRF tokens are rejected and the protected row is unchanged; a correct token persists the change
- [ ] #3 Sign-out and session revocation invalidate the session row and the next request is unauthenticated
- [ ] #4 Operator and Viewer admin mutations are rejected with no protected-row change; the Viewer browser shows no usable admin mutation
- [ ] #5 Registration after the first user is checked against allow_registration semantics; a failure is recorded as a linked product defect
- [ ] #6 Both scenarios executed in real VMs; commands, revision, outcomes, and evidence paths are in task notes
<!-- AC:END -->
