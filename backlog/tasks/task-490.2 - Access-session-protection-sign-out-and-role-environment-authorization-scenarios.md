---
id: TASK-490.2
title: >-
  Access: session protection, sign-out, and role/environment authorization
  scenarios
status: Backlog
assignee: []
created_date: '2026-10-17 16:34'
updated_date: '2026-10-18 13:45'
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
Owning check: integration-access. Contract IDs: access.lifecycle.sessions, access.lifecycle.authorization, access.lifecycle.registration-closed. This packet owns disjoint case fragments only: `checks/integration-access/scenarios/fragments/task-490.2-sessions.py/.cjs` and `task-490.2-authorization.py/.cjs`. The coordinator owns `lifecycle-local-bootstrap.nix`, the composed Python entry point, browserSpecs mapping, and owner registration. Do not create a packet-specific NixOS derivation.

Compose local-auth cases after the real TASK-490.1 first-admin bootstrap in the same fresh VM/database. Reuse the created Admin and generated identity receipts; create Operator/Viewer through the real API/UI. Assert missing/mismatched CSRF is 403 without protected-row mutation, correct CSRF persists, sign-out/revocation invalidates the selected session, and denied role mutations leave rows unchanged. Check closed registration and cross-environment denial only under accepted owner authority; uncertain cases stay blocked: decision. Preserve browser navigation and exact identity assertions.

The base scenario passed at `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`; TASK-490.25/.26 are integrated. Remaining dependency is TASK-490.1 review/selection; no other role helper is needed. The controller/browser interface and same-scenario placement are documented. Focused verification after composition: `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`; aggregate: `nix build .#checks.x86_64-linux.integration-access -L`. Neither has run with these fragments, so keep this packet Backlog until selected and accepted-authority cases are concrete.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Session, CSRF, sign-out and revocation cases are composed into integration-access/scenarios/lifecycle-local-bootstrap by the coordinator; no packet-specific VM derivation is added
- [ ] #2 Missing/mismatched CSRF is rejected without protected-row changes; correct tokens persist the mutation
- [ ] #3 UI sign-out and session revocation invalidate the correct session row and the next request is unauthenticated
- [ ] #4 Operator/Viewer admin mutations are rejected without row changes; unresolved environment scope remains blocked pending accepted authority
- [ ] #5 Closed registration is tested only against accepted requirements; correct failures retain assertions and linked product defects
- [ ] #6 The coordinator's real VM run executes these cases with exact tested revision, command, result and evidence recorded
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old paths, commands, discovery and D1 authority instructions. Actual foundation commit is unresolved; two existing Rust package tests block foundation and no VM execution occurred. Backlog, not ready: actual dependency commit(s) and verified commands are required before To Do. No Luna delegation or executed slice is claimed. Coordinator owns shared lib, owner default.nix registration, CI/docs; this child writes only its disjoint scenario files.

Rebase verification: the TASK-490.1 base scenario passed at rebased source `bba718befbc183863781656df2df272e5f735d5a`; command remains `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`. This fragment set is still Backlog because TASK-490.1 remains In Progress and the role/environment/closed-registration assertions require owner authority where not already accepted.
<!-- SECTION:NOTES:END -->
