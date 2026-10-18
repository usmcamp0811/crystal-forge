---
id: TASK-490.2
title: >-
  Access: session protection, sign-out, and role/environment authorization
  scenarios
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:34'
updated_date: '2026-10-18 14:11'
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
modified_files:
  - checks/integration-access/scenarios/fragments/task-490.2-sessions.py
  - checks/integration-access/scenarios/fragments/task-490.2-sessions.cjs
  - checks/integration-access/scenarios/fragments/task-490.2-authorization.py
  - checks/integration-access/scenarios/fragments/task-490.2-authorization.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 514000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-access. Contract IDs: access.lifecycle.sessions, access.lifecycle.authorization, access.lifecycle.registration-closed. Goal: replace mocked session/role proofs with real requests, browser actions and authoritative persistence checks. Risk: medium (security assertions). This is test-only work; OIDC (.6), forms (.3), navigation (.4), TTL timing and product changes are out of scope.

Writable paths (only):
- checks/integration-access/scenarios/fragments/task-490.2-sessions.py
- checks/integration-access/scenarios/fragments/task-490.2-sessions.cjs
- checks/integration-access/scenarios/fragments/task-490.2-authorization.py
- checks/integration-access/scenarios/fragments/task-490.2-authorization.cjs

Composition target: checks/integration-access/scenarios/lifecycle-local-bootstrap.nix. The coordinator owns its Python composition, browserSpecs mapping and explicit owner registration/aggregation. No packet-specific NixOS derivation. Run after the real first-admin bootstrap in the same fresh VM/database. Earlier real records in this scenario MAY be consumed through receipts; another derivation's live VM, database, cookies, IDs or receipts MUST NOT be consumed.

The user SELECTED the session portion. Leave status changes to the coordinator. Its concrete prerequisite is tested foundation bba718befbc183863781656df2df272e5f735d5a (bba718be), based on origin/dev 5ae7feab8a7b372e8efe0d2dd21b6f53f0625064, with TASK-490.25/.26 integrated. The controller-only session proofs do not require services/builder/target roles, completion of future dependencies, or unresolved environment/registration authority. Use FACTS["admin"] for authored credentials and receipts()["admin.userId"] for generated identity. Existing driver interfaces are case, expect, expect_equal, receipt, receipts, wait_for, sql_json(controller, query), http(controller, method, path, body=None, session=None, csrf=False, base=...), and run_browser(controller, spec, viewport="desktop", base_url=...). Named http session jars persist independently; csrf=True sends the jar's double-submit token. A mismatched-header probe must use a concrete request without changing shared helpers. Each run_browser invocation starts an independent browser context; sign in within that context and pass identities through receipts, never assume bootstrap cookies carry over. Browser helpers include page/expect/facts/receipts/emit/url/step/screenshot/whoami.

Session cases: create two real sessions and use DELETE /api/v1/user/sessions/:id against the selected other session as the protected mutation for all three CSRF probes. Record the exact selected session row and its invalidated_at value before each probe. An absent x-csrf-token header returns 403 and leaves that row unchanged; a mismatched header returns 403 and leaves that row unchanged. Valid CSRF returns 204, invalidates only the selected session and sets its invalidated_at. The revoked session's next protected operation returns 401 while the retained session still works. The endpoint returns a plain-text CSRF error; assert status and exact row effects without requiring a JSON body or a specific structured error code. UI sign-out invalidates its exact session; whoami is false after reload and protected navigation redirects to /login. Rejected requests create no session or protected mutation.

Remaining authorization portion: create Operator/Viewer through the real admin API/UI with CSRF. Denied admin create/PATCH/config mutations return 403 without row changes; Viewer has no usable admin mutation, including direct /admin access. Cross-environment denial and closed registration after an existing user with allow_registration=false require accepted endpoint/scope/registration semantics. Only those uncertain assertions stay blocked: decision. Their authority gaps do not block the selected session portion.

Authority/source reading: the explicit current user request is the accepted source authority for the session proofs, including the selected-session DELETE CSRF probes, status codes and exact row effects. Accepted owner requirements control the remaining authorization expectations; knowledge pages are as-built evidence, not D1 acceptance authority. Read security/session-cookies-and-csrf.md, security/api-authentication-and-authorization.md and ui/admin-console-and-login-views.md under docs/knowledge; cf-server auth/session.rs, handlers/api/auth_session.rs, user_sessions.rs, rbac.rs, admin.rs (CreateAdminUserRequest), auth_local.rs (register/allow_registration), auth_status.rs, server routes and migration 0071_auth_identity_rbac.sql; web-ui login/register/sidebar. Author role/environment facts independently (design users by ID, environments by name); never use a response under test as its own expected value.

Focused verification after coordinator composition: `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`. The base passed; these fragments have not run. Record tested revision, actual exit/result and evidence (result.json, browser/<spec>/, journals/controller/); retain correct failures and linked product defects. Escalate shared-interface/authority gaps to the coordinator without weakening assertions or editing production/shared/registration/CI/design files.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The coordinator composes only the named session/authorization fragments into lifecycle-local-bootstrap after real bootstrap; no packet-specific VM derivation is added
- [ ] #2 DELETE /api/v1/user/sessions/:id is the protected mutation for all three CSRF probes: missing header returns 403 with the exact selected session row/invalidated_at unchanged; mismatched header returns 403 with that row/invalidated_at unchanged; valid CSRF returns 204 and invalidates only the selected session; its next protected operation returns 401 while the retained session still works; assert status and row effects without requiring JSON or a specific structured CSRF error code
- [ ] #3 Two real sessions prove selected-session revocation and retained-session validity; UI sign-out invalidates its exact row, next request/reload is unauthenticated and protected navigation redirects to /login
- [ ] #4 Operator/Viewer admin mutations are rejected without row changes and Viewer has no usable direct/UI admin mutation; only uncertain environment-scoping assertions remain blocked pending accepted authority
- [ ] #5 Closed registration/no new user is asserted against accepted owner semantics; unresolved registration authority blocks only that assertion and does not block the user-selected session portion
- [ ] #6 The coordinator executes the focused lifecycle-local-bootstrap gate with these fragments and records exact tested revision, command, outcome and evidence; identities use FACTS/admin.userId receipts and independent authenticated contexts
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Use the bootstrap-created Admin and receipts after the five existing cases. Create two named real HTTP sessions and identify each current session via API + DB.
2. Exercise DELETE of the second session without CSRF, with mismatched CSRF, and with valid CSRF. Compare authored invalidated=false/true expectations and exact session rows before/after; prove revoked-session request is 401 and retained session stays valid. This action itself is the protected mutation, so no unrelated config changes are needed.
3. Add independent browser sign-in/profile Sign out proof. Emit its current session ID, assert logout and reload/protected navigation unauthenticated, then prove that exact DB row invalidated.
4. Coordinator composes only these fragments in current scenario; run DB targeted test first then one combined VM and owner reuse. Publish artifacts on MR. Leave authorization/registration obligations open.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: the user selected the sessions portion against bba718be; coordinator owns status transition and composition. Authorization authority gaps remain limited to the affected assertions. No fragment run or completion is claimed.

### Historical
The 2026-10-17 organization review found unresolved foundation/package prerequisites and no VM execution at that time. The former separate session/role scenario proposal was replaced by same-scenario fragments. Earlier bootstrap passed at b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56. After rebase, the actual tested prerequisite is bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab8a7b372e8efe0d2dd21b6f53f0625064. Its recorded runtime is riky1mm0nn185wb26abw7zzshnbml0fp (292 migrations). These are foundation results, not execution of this packet.

User selected the session portion in the current assignment. Execute it in the existing foundation worktree as explicitly requested, not a separate worker worktree. The tested controller prerequisite is bba718befbc183863781656df2df272e5f735d5a; unresolved role/environment/registration assertions do not block this session subset.

LOCK: openai on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation. User explicitly selected session subset and existing worktree. The protected mutation is selected-session revocation; 403 status and unchanged rows are the user-required security contract (no generic structured error-code requirement on this endpoint).
<!-- SECTION:NOTES:END -->
