---
id: TASK-490.2
title: >-
  Access: session protection, sign-out, and role/environment authorization
  scenarios
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:34'
updated_date: '2026-10-19 02:13'
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
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
modified_files:
  - checks/integration-access/scenarios/lifecycle-local-bootstrap.nix
  - checks/integration-access/scenarios/fragments/sessions.py
  - checks/integration-access/scenarios/fragments/sessions.cjs
  - checks/integration-access/scenarios/fragments/authorization.py
  - checks/integration-access/scenarios/fragments/authorization.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 514000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-access. Contract IDs: access.lifecycle.sessions, access.lifecycle.authorization, access.lifecycle.registration-closed. Goal: replace mocked session/role proofs with real requests, browser actions and authoritative persistence checks. Risk: medium (security assertions). The user-selected session subset and the selected Operator/Viewer admin-denial cases are implemented, executed and verified. The whole task remains In Progress because the cross-environment scope rule and the closed-registration policy remain open. Non-goals: OIDC (.6), forms (.3), navigation (.4), TTL timing and product changes.

Writable paths (only):
- checks/integration-access/scenarios/fragments/sessions.py
- checks/integration-access/scenarios/fragments/sessions.cjs
- checks/integration-access/scenarios/fragments/authorization.py
- checks/integration-access/scenarios/fragments/authorization.cjs

Composition target: checks/integration-access/scenarios/lifecycle-local-bootstrap.nix. Coordinator owns Python composition, browserSpecs mapping and explicit owner registration/aggregation. The session fragment and then the authorization fragment execute after the five real first-admin bootstrap cases in the same fresh VM/database. No packet-specific derivation, owner or flake check exists. Earlier same-scenario real records MAY be consumed through receipts; another derivation's live VM, database, cookies, IDs or receipts MUST NOT be consumed.

Interfaces: FACTS["admin"], receipts()["admin.userId"], case, expect, expect_equal, receipt, receipts, wait_for, sql_json(controller, query), http(controller, method, path, body=None, session=None, csrf=False, base=...) and run_browser(controller, spec, viewport="desktop", base_url=...). Named HTTP jars persist independently; csrf=True sends the double-submit token from the jar through a VM header file, so the token is never logged. Browser helpers include page/expect/facts/receipts/emit/url/step/screenshot/whoami.

Verified session cases (five): two real sessions exercise DELETE /api/v1/user/sessions/:id. Missing or mismatched x-csrf-token returns 403 with rows unchanged; valid CSRF returns 204 and invalidates only the selected session; its next operation returns 401 while the retained session works; production UI sign-out invalidates its exact session.

Verified authorization cases (six), authority: the approved design's role catalogue (Admin: "Manage users & OIDC", "Edit server config") and its Server Management page ("Admin-only"); the as-built docs document the 403 `forbidden` response. They do not define Operator as generally read-only. (1) The Admin API creates an Operator (design u2), a Viewer (u4) and a protected target (u7); persisted roles are verified; each actor signs in through the real login and whoami confirms id and role. (2) Admin sends every probe payload first and the state is restored: retry policy 200, classification banner 200, escalation create 201 then delete 204, escalation PATCH 200 then restore 200. (3) Operator and Viewer POST /api/v1/admin/users with a role-escalation payload: 403 forbidden, no account created. (4) Each PATCHes another account and its own with {role: Admin, enabled: false}: 403 forbidden, no change. (5) Each PUTs /api/v1/admin/automatic-retry-policy and /api/v1/admin/classification-config: 403 forbidden, values unchanged. (6) An independent Viewer browser context signs in, finds no /admin navigation entry, sees the production Access Denied state at /admin, finds none of the Admin mutation buttons and issues no non-GET admin request. Every denied request carries valid session cookies and a matching CSRF header, and actor identity and role are verified immediately before it. Protected state compared before and after each probe: users (identity, enablement, password digest), role assignments, environment memberships, user-administration audit counts, retry policy, banner. Each denial is recorded as a receipt.

Not covered by the selected cases: Operator browser view; other Admin routes (OIDC mappings, user DELETE as a non-Admin, heartbeat configuration); visual comparison. Cross-environment denial is blocked only where the exact endpoint/scope rules lack accepted authority. Closed registration (an existing user with allow_registration=false creates no new user) remains blocked pending the accepted registration-policy decision, and no case exists yet. Those decision gaps apply only to their cases.

Source reading retained: docs/knowledge/security/session-cookies-and-csrf.md, security/api-authentication-and-authorization.md, ui/admin-console-and-login-views.md; cf-server auth/session.rs, handlers/api/auth_session.rs, user_sessions.rs, rbac.rs, admin.rs, auth_local.rs, routes in bin/server.rs and migrations 0071, 0073, 0142, 0189; web-ui views/admin.rs and components/layout/app_shell.rs. Retain correct failures and escalate helper/authority gaps without editing production/shared/CI/design files.

Verification: `sh packages/ci/integration-owner-build.sh integration-access` PASS at 8a857bc872cf331a19036b97a93f2b7741e9728b, VM executed newly, 16 of 16 cases (ten earlier then six authorization). Evidence /nix/store/xld57xqbfpvqpcmbqjij5djsm9l2j0mv-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The coordinator composes only the named session/authorization fragments into lifecycle-local-bootstrap after real bootstrap; no packet-specific VM derivation, owner or flake check is added
- [x] #2 DELETE /api/v1/user/sessions/:id probes prove missing/mismatched CSRF each returns 403 with the exact selected row/invalidated_at unchanged; valid CSRF returns 204 and invalidates only the selected session; its next protected operation returns 401 while the retained session works; status/row effects are required, not JSON or a structured CSRF code
- [x] #3 Two real sessions prove selected-session revocation and retained-session validity; UI sign-out invalidates its exact row, next request/reload is unauthenticated and protected navigation redirects to /login
- [x] #4 Operator and Viewer admin-only user create, user PATCH and server-configuration mutations are rejected with 403 and no protected change, and Viewer has no usable direct/UI admin mutation (selected cases executed); only the uncertain exact cross-environment endpoint/scope rule is decision-blocked and remains unimplemented
- [ ] #5 Closed registration/no new user is asserted against accepted owner semantics; unresolved registration-policy authority blocks only that assertion, not the completed session or admin-denial cases
- [x] #6 Coordinator executes lifecycle-local-bootstrap with the implemented session and authorization fragments and records exact tested revision, command, outcome and evidence; identities use FACTS/receipts and independently authenticated contexts; unimplemented cases are explicitly excluded from the result
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Completed: reused bootstrap Admin facts/receipt and created two named real HTTP sessions, identifying each exact row through API/DB.
2. Completed: session DELETE probes (missing/mismatched/valid CSRF), retained-session validity and independent browser sign-out.
3. Completed: Admin API creates Operator, Viewer and a protected target; actors sign in; Admin controls prove each probe payload valid and restore state.
4. Completed: Operator and Viewer admin-only user create, PATCH (other and own account) and server-configuration PUT return 403 forbidden with unchanged protected state; Viewer browser shows no admin entry and Access Denied at /admin.
5. Completed: coordinator composition and PASS (16 of 16) at 8a857bc872cf331a19036b97a93f2b7741e9728b with the VM executed newly.
6. Remaining: cross-environment denial (needs the accepted endpoint/scope rule) and closed registration (needs the accepted policy). Keep the task In Progress.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Current (2026-10-18)

Status: In Progress. AC #1, #2, #3, #4 and #6 are checked. AC #5 (closed registration) stays open: it needs the accepted registration-policy decision and no case exists. The exact cross-environment endpoint/scope rule also awaits its decision and has no case. Each decision blocks only its own case.

Implemented and executed under the owner's explicit selections, in the existing scenario (no new owner, derivation or flake check): five session cases and six Operator/Viewer authorization cases, composed after the five bootstrap cases. Authority for the authorization cases: the approved design's role catalogue and "Admin-only" Server Management page; the as-built docs document the 403 `forbidden` response.

Tested source `8a857bc872cf331a19036b97a93f2b7741e9728b` on a clean tree. Command `sh packages/ci/integration-owner-build.sh integration-access` exited 0. The VM executed newly (25 derivations built including the scenario run; driver log shows `source 8a857bc8…`); wall time about 110 s, case time 29.1 s. `result.json`: status passed, 16 of 16 cases; the ten earlier cases are unchanged and first, then the six authorization cases. Evidence `/nix/store/xld57xqbfpvqpcmbqjij5djsm9l2j0mv-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run`; aggregate `/nix/store/4fqzi6xrlk5vz5w6shmlflvkcazi6hy9-integration-access`.

Receipts in `result.json`: authorization.operator.userId f6abcf83-8467-47ca-9f6a-ca0f85eff663, authorization.viewer.userId 2f1bf363-f52b-47e5-9a5d-c72aa8ad829c (equal to authorization.viewer.browserUserId), authorization.target.userId a4166a9b-62f1-42b7-b4d4-335a4a89be5d, ten `authorization.denied.<actor>.<operation>` entries (each status 403, error forbidden, stateUnchanged true), `authorization.adminControls` (200, 200, 201, 204, 200, 200) and `authorization.protectedStateBaseline` (4 users, 4 role assignments, 0 environment memberships; audit counts match the Admin control sequence, which shows the comparison sees real changes).

Hosted: the GitLab `integration-access` job passed all 16 cases in MR pipeline 2960613754 at `08a79037f872ec7743293a9d973a21f9aa4acebb` (58 derivations built; 74 s; 1.0 MB artifact).

Viewer browser result passed at /admin; the screenshot shows the Viewer, a navigation without a Server entry, and the Access Denied section. No real runtime token appears in the build log or evidence. No product defect was found and no production, UI, API, module, migration or schema file changed.

Reviewer artifacts: scenario result https://gitlab.com/-/project/70402481/uploads/678acd9a2c79d87d72d81c70cad2abd4/result.json ; Viewer browser result https://gitlab.com/-/project/70402481/uploads/dc58018286300818fd347b58b862aea5/viewer-admin-denied-result.json ; screenshot https://gitlab.com/-/project/70402481/uploads/e840b7c9cd5b2bfa1222deea8e788826/viewer-admin-access-denied.png . Review: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335

Not covered: Operator browser view; other Admin routes (OIDC mappings, non-Admin user DELETE, heartbeat configuration); visual comparison.

LOCK: openai on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation.

## Historical (superseded readiness statements)

Earlier state: AC #1 was open because the authorization fragment was not composed, and the Operator/Viewer cases were unimplemented. Final source `81d65be745cc9f9777d368900d01fe0f207cb052` passed the focused gate and aggregate with 10 of 10 cases (five bootstrap, five sessions), 292 migrations and database.unixSocketClients=22 (evidence `/nix/store/i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh-…`). Receipts then: admin.userId 0560a10d-20c1-4464-a174-9b1e1eddbaf4, sessions.revokedId 9a4e59eb-450b-4105-b54b-6514ba9d307a, sessions.retainedId 4734fef6-c3d6-43ee-9a5d-9f2bc66305b0, sessions.browserSignOutId ad65734a-ba57-4935-9273-a70a987b749b. The coordinator reported database_url_tests 2/2, production checks 1830 passed / 0 failed / 767 ignored, and okf validation 58 tests, 192 concepts, 216 Mermaid blocks. Artifacts: https://gitlab.com/-/project/70402481/uploads/2ab7a8febcfb1aa1386364464decc2f2/result.json ; https://gitlab.com/-/project/70402481/uploads/5d551029e97b8b3dd534f312deb35b5f/result.json ; https://gitlab.com/-/project/70402481/uploads/ec258b80b8a107c27d32b9f5c4cf3109/signed-out-protected-navigation.png .

The 2026-10-17 organization review found unresolved foundation/package prerequisites and no VM execution at that point. Separate session/role scenario proposals became same-scenario fragments. Bootstrap passed at b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56, then rebased bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab8a7b372e8efe0d2dd21b6f53f0625064; runtime riky1mm0nn185wb26abw7zzshnbml0fp recorded 292 migrations. The first session browser attempt at 53718683 failed due to the Coach overlay and an unhandled promise; normal UI Close coach and Promise.all corrected the test. TASK-492 remains unchanged.
<!-- SECTION:NOTES:END -->
