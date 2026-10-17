---
id: TASK-490.2
title: >-
  Access: session protection, sign-out, and role/environment authorization
  scenarios
status: Backlog
assignee: []
created_date: '2026-10-17 16:34'
updated_date: '2026-10-17 17:25'
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
  - checks/integration-access/scenarios/lifecycle-local-bootstrap.nix
parent_task_id: TASK-490
priority: high
type: task
ordinal: 514000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-access. Contract IDs: access.lifecycle.sessions, access.lifecycle.authorization, access.lifecycle.registration-closed. Session and role boundaries currently rely on mocked steps (13i, 15i, 14d) and isolated Rust tests; this task proves rejected actions have no server-side effect.

Exclusive writes: checks/integration-access/scenarios/lifecycle-session-protection.{nix,py,cjs} and lifecycle-role-restrictions.{nix,py,cjs}. Everything else is read-only. Non-goals: OIDC (490.6), forms (490.3), navigation (490.4), TTL timing, production changes. Follow TASK-490 test-only restrictions.

Corrected organization: checks/integration-access/default.nix is coordinator-owned, uses { lib, inputs, pkgs, ... }, explicitly registers scenarios, and pkgs.linkFarm aggregates ALL registered gates. Shared helpers: lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }. Scripts/fixtures/assets remain owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Child integration requires coordinator registration, never child edits to shared lib, owner default.nix, CI or docs. Meaningful WIP may be discoverable. Only Access exists now; no placeholder owners.

Authority review: security/session-cookies-and-csrf.md, security/api-authentication-and-authorization.md and ui/admin-console-and-login-views.md under docs/knowledge are as-built evidence only, not accepted D1 authority. Do not label scenario headers Authority: as-built. Obtain accepted owner requirements for uncertain cases; those cases stay blocked pending owner decision.

Read-only source: cf-server src/auth/session.rs (__Host-cf-session, __Host-cf-csrf, x-csrf-token); handlers/api/auth_session.rs (logout, validate_csrf, RequireCsrf), user_sessions.rs (list/revoke/revoke-all), rbac.rs, admin.rs (CreateAdminUserRequest: email, display_name, password, role, environments), auth_local.rs (register does not read allow_registration), auth_status.rs; server auth routes; migration 0071_auth_identity_rbac.sql; web-ui login/register/sidebar.

Use the actual committed foundation interface, not planned signatures. Proposed pattern is checks/integration-access/scenarios/lifecycle-local-bootstrap.{nix,py,cjs}. Required helpers to verify in the future packet: controller browser role, case/expect/expect_equal/receipt/receipts/wait_for/http/sql_json/run_browser and browser page/expect/facts/receipts/emit/url/step/screenshot/whoami. API setup: POST /api/auth/local/register then login with a named session; create Operator/Viewer through POST /api/v1/admin/users with CSRF. Design facts localUserFromDesign u1 admin, u2 operator, verified viewer id (e.g. u4); key by id with byId, environments by name. Authored staging/production scope values.

lifecycle-session-protection: absent CSRF header -> 403 csrf_validation_failed, protected mutation row unchanged (e.g. PUT /api/v1/admin/classification-config); mismatched token likewise; correct token succeeds and persists. Two sessions: DELETE /api/v1/user/sessions/:id revokes the other, invalidated_at set, next request unauthenticated. Browser sign-in then UI sign-out: whoami false after reload, session invalidated, protected route redirects /login. Rejected requests do not mutate rows or create sessions.

lifecycle-role-restrictions: Operator/Viewer admin create/PATCH/config mutations return 403 with rows unchanged. Assert cross-environment denial only where accepted authority defines the endpoint/scoping; otherwise block pending owner decision. Viewer UI has approved unavailable/absent admin actions and no usable mutation controls on direct /admin; no mutation succeeds. With default allow_registration=false and an existing user, check closed registration/no new user only against accepted owner semantics; uncertain semantics stay blocked. Preserve correct failing assertions and link product bugs.

Candidate commands, NOT verified: nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-session-protection -L and corresponding lifecycle-role-restrictions; append .run for evidence. Coordinator aggregate: nix build .#checks.x86_64-linux.integration-access -L. Explicit registry inspection: nix eval .#checks.x86_64-linux.integration-access.scenarios --apply builtins.attrNames. Run one VM test at a time until measured capacity supports a higher limit. Verify actual evidence layout in the dependency commit (proposed evidence/result.json, browser/<spec>/, journals/controller/).

Completion requires explicit coordinator registration and real VM execution with exact revision, commands, outcomes and evidence store paths. Independent expectations come from authored facts/accepted requirements, not CF responses. Correct product failures keep assertions and linked Backlog bug (contract ID, app revision, minimal steps, expected/actual, evidence). Shared-interface gaps and authority conflicts go to the coordinator; never weaken, skip, retry away, or fix product code.

READINESS: actual foundation dependency commit unresolved; foundation blocked by two existing Rust package tests, no VM execution. This task remains Backlog. Before To Do, record actual dependency commit(s), implemented signatures, fixtures, disjoint paths, registration and verified commands. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both scenarios exist only under checks/integration-access/scenarios/ and the coordinator explicitly registers them in checks/integration-access/default.nix; the normal pkgs.linkFarm aggregate includes both gates
- [ ] #2 Missing/mismatched CSRF tokens are rejected without protected-row changes; correct tokens persist the mutation
- [ ] #3 UI sign-out and session revocation invalidate the session row and the next request is unauthenticated
- [ ] #4 Operator/Viewer admin mutations are rejected without row changes; Viewer has no usable admin mutation; uncertain environment-scoping requirements remain blocked pending owner decision
- [ ] #5 Closed registration is checked against accepted owner requirements, not as-built D1 authority; correct failures retain assertions and linked product defects, uncertain semantics remain blocked
- [ ] #6 Both scenarios execute in real VMs with commands, revision, outcomes and .run evidence in task notes after an actual dependency commit and verified packet exist
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old paths, commands, discovery and D1 authority instructions. Actual foundation commit is unresolved; two existing Rust package tests block foundation and no VM execution occurred. Backlog, not ready: actual dependency commit(s) and verified commands are required before To Do. No Luna delegation or executed slice is claimed. Coordinator owns shared lib, owner default.nix registration, CI/docs; this child writes only its disjoint scenario files.
<!-- SECTION:NOTES:END -->
