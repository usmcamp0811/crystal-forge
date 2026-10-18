---
id: TASK-490.2
title: >-
  Access: session protection, sign-out, and role/environment authorization
  scenarios
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:34'
updated_date: '2026-10-18 14:40'
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
Owning check: integration-access. Contract IDs: access.lifecycle.sessions, access.lifecycle.authorization, access.lifecycle.registration-closed. Goal: replace mocked session/role proofs with real requests, browser actions and authoritative persistence checks. Risk: medium (security assertions). The user-selected session subset is implemented and verified; the whole task remains In Progress because authorization/registration obligations remain open. Non-goals: OIDC (.6), forms (.3), navigation (.4), TTL timing and product changes.

Writable paths (only):
- checks/integration-access/scenarios/fragments/task-490.2-sessions.py
- checks/integration-access/scenarios/fragments/task-490.2-sessions.cjs
- checks/integration-access/scenarios/fragments/task-490.2-authorization.py
- checks/integration-access/scenarios/fragments/task-490.2-authorization.cjs

Composition target: checks/integration-access/scenarios/lifecycle-local-bootstrap.nix. Coordinator owns Python composition, browserSpecs mapping and explicit owner registration/aggregation. The session fragments execute after the five real first-admin bootstrap cases in the same fresh VM/database; authorization fragments are not implemented/composed. No packet-specific derivation. Earlier same-scenario real records MAY be consumed through receipts; another derivation's live VM, database, cookies, IDs or receipts MUST NOT be consumed.

Final verified source is 81d65be745cc9f9777d368900d01fe0f207cb052, with TASK-490.1 now Review and .25/.26 integrated. The session proofs need no future services/builder/target roles or global foundation-completion gate. Use FACTS["admin"] for authored credentials and receipts()["admin.userId"] for identity. Concrete driver interfaces remain case, expect, expect_equal, receipt, receipts, wait_for, sql_json(controller, query), http(controller, method, path, body=None, session=None, csrf=False, base=...) and run_browser(controller, spec, viewport="desktop", base_url=...). Named HTTP jars persist independently; csrf=True sends the double-submit token. Mismatched-header probes use a concrete request without changing shared helpers; header-file handling redacts CSRF secrets. Each run_browser invocation signs in within an independent context and exchanges generated IDs through receipts, not inherited bootstrap cookies. Browser helpers include page/expect/facts/receipts/emit/url/step/screenshot/whoami.

Verified session cases: two real sessions exercise DELETE /api/v1/user/sessions/:id as the protected mutation. Missing x-csrf-token returns 403 with the exact selected row/invalidated_at unchanged; mismatched token returns 403 with that row unchanged; valid CSRF returns 204, invalidates only the selected session and sets invalidated_at. Its next protected operation returns 401 while the retained session works. The endpoint's plain-text CSRF error imposes no JSON/structured-code requirement. Production UI sign-out invalidates its exact session; reload/whoami and protected navigation prove unauthenticated state and /login redirect. Rejected requests create no session or protected mutation. Normal Close coach interaction and Promise.all make the browser proof deterministic without product changes.

Remaining authorization work: Operator/Viewer admin create/PATCH/config denial and Viewer direct/UI admin-mutation cases are unimplemented, not categorically authority-blocked. Create these users through the real API/UI with CSRF and assert 403/no row effects against accepted requirements. Cross-environment denial is blocked only where the exact endpoint/scope rules lack accepted authority. Closed registration/no new user with an existing user and allow_registration=false remains blocked pending the accepted registration-policy decision. Those decision gaps apply only to their cases; they do not invalidate or block the completed session subset.

Authority/source reading: the explicit current user request is the accepted session source authority, including DELETE probes, status codes and exact row effects. Accepted owner requirements control remaining authorization expectations; knowledge pages are as-built evidence, not D1 acceptance authority. Retain source references: docs/knowledge/security/session-cookies-and-csrf.md, security/api-authentication-and-authorization.md, ui/admin-console-and-login-views.md; cf-server auth/session.rs, handlers/api/auth_session.rs, user_sessions.rs, rbac.rs, admin.rs (CreateAdminUserRequest), auth_local.rs (register/allow_registration), auth_status.rs, routes and migration 0071_auth_identity_rbac.sql; web-ui login/register/sidebar. Author role/environment facts independently, with design users by ID and environments by name; do not make responses under test their own expectations.

Verification: focused `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L` and owner aggregate both PASS at 81d65be745cc9f9777d368900d01fe0f207cb052. Evidence /nix/store/i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/result.json records all 10 cases: five bootstrap then five session cases, 292 migrations, database.unixSocketClients=22 and exact identity receipts. Session evidence is linked to existing draft MR !335; no whole-task Review/completion claim. Retain correct failures and escalate helper/authority gaps without editing production/shared/registration/CI/design files.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The coordinator composes only the named session/authorization fragments into lifecycle-local-bootstrap after real bootstrap; no packet-specific VM derivation is added; the authorization-fragment composition remains open until implemented
- [x] #2 DELETE /api/v1/user/sessions/:id probes prove missing/mismatched CSRF each returns 403 with the exact selected row/invalidated_at unchanged; valid CSRF returns 204 and invalidates only the selected session; its next protected operation returns 401 while the retained session works; status/row effects are required, not JSON or a structured CSRF code
- [x] #3 Two real sessions prove selected-session revocation and retained-session validity; UI sign-out invalidates its exact row, next request/reload is unauthenticated and protected navigation redirects to /login
- [ ] #4 Operator/Viewer admin mutations are rejected without row changes and Viewer has no usable direct/UI admin mutation; these cases remain unimplemented; only uncertain exact environment endpoint/scope rules are decision-blocked
- [ ] #5 Closed registration/no new user is asserted against accepted owner semantics; unresolved registration-policy authority blocks only that assertion, not the completed session subset
- [x] #6 Coordinator executes lifecycle-local-bootstrap with the implemented session fragments and records exact tested revision, command, outcome and evidence; identities use FACTS/admin.userId receipts and independently authenticated contexts; unimplemented authorization is explicitly excluded from this result
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Completed: reused bootstrap Admin facts/receipt and created two named real HTTP sessions, identifying each exact row through API/DB.
2. Completed: DELETE revocation without CSRF and with mismatched CSRF returned 403/no row effects; valid CSRF returned 204 and invalidated only the selected session; its next operation returned 401 while the retained session remained valid.
3. Completed: independent production-browser sign-in/profile Sign out emitted its exact session ID; logout, reload and protected navigation proved unauthenticated state and the exact row was invalidated. Normal Close coach interaction and Promise.all fixed test orchestration; CSRF header files are redacted.
4. Completed for sessions: coordinator composition and focused/aggregate PASS at 81d65be745cc9f9777d368900d01fe0f207cb052; targeted DB proof passed and stable scenario/browser/screenshot artifacts were uploaded for draft MR !335.
5. Remaining: implement Operator/Viewer authorization fragments and their composition. Resolve only the exact cross-environment endpoint/scope rules and closed-registration policy cases that need accepted decisions. Keep whole task In Progress.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Session subset complete and verified; whole TASK-490.2 remains In Progress. AC #2/#3/#6 are checked. AC #1 remains unchecked because its authorization-fragment composition is not implemented; AC #4/#5 remain open. Operator/Viewer cases are unimplemented, not necessarily authority-blocked. Only uncertain exact cross-environment endpoint/scope and closed-registration policy cases require decisions. TASK-490.1 Review and future role dependencies do not block completed controller session proofs.

LOCK: openai on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation. Session subset is awaiting review within draft MR !335; remaining authorization work is still tracked here.

Final source 81d65be745cc9f9777d368900d01fe0f207cb052: focused `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L` and aggregate `nix build .#checks.x86_64-linux.integration-access -L` PASS. Evidence /nix/store/i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/result.json confirms 10/10 cases (first five bootstrap, next five sessions), 292 migrations and database.unixSocketClients=22. Exact receipts:
- admin.userId: 0560a10d-20c1-4464-a174-9b1e1eddbaf4
- sessions.revokedId: 9a4e59eb-450b-4105-b54b-6514ba9d307a; sessions.revocationPersisted=true
- sessions.retainedId: 4734fef6-c3d6-43ee-9a5d-9f2bc66305b0
- sessions.browserSignOutId: ad65734a-ba57-4935-9273-a70a987b749b; sessions.browserSignOutPersisted=true

The coordinator reports targeted `SQLX_OFFLINE=true nix develop -c cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib database_url_tests` PASS 2/2 including SQLx password rebuilt URL; enabled production checks 1830 passed, 0 failed, 767 ignored; existing okf validation 58 tests, 192 concepts, 216 Mermaid blocks passed. These record edits did not rerun checks; final result.json was directly read.

Review: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335
Scenario: https://gitlab.com/-/project/70402481/uploads/2ab7a8febcfb1aa1386364464decc2f2/result.json
Browser: https://gitlab.com/-/project/70402481/uploads/5d551029e97b8b3dd534f312deb35b5f/result.json
Screenshot: https://gitlab.com/-/project/70402481/uploads/ec258b80b8a107c27d32b9f5c4cf3109/signed-out-protected-navigation.png

### Historical
The 2026-10-17 organization review found unresolved foundation/package prerequisites and no VM execution at that point. Separate session/role scenario proposals became same-scenario fragments. Bootstrap passed at b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56, then rebased bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab8a7b372e8efe0d2dd21b6f53f0625064; runtime riky1mm0nn185wb26abw7zzshnbml0fp recorded 292 migrations. Those were bootstrap-only results, before the selected sessions ran.

The user selected sessions in the existing foundation worktree; no separate worker worktree or unresolved role/environment/registration authority was required for this subset. The user explicitly required selected-session DELETE status/row effects rather than a generic structured error-code requirement.

The first browser attempt at 53718683 failed due to the Coach overlay and an unhandled promise. Normal UI Close coach and Promise.all corrected the test; CSRF header-file redaction prevents secret leakage. No production fix or new product defect was needed. All five session cases pass in the final result; TASK-492 remains unchanged.
<!-- SECTION:NOTES:END -->
