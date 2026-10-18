---
id: TASK-490.2
title: >-
  Access: session protection, sign-out, and role/environment authorization
  scenarios
status: Backlog
assignee: []
created_date: '2026-10-17 16:34'
updated_date: '2026-10-18 02:37'
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
Owning check: integration-access. Contract IDs: access.lifecycle.sessions, access.lifecycle.authorization, access.lifecycle.registration-closed. This packet owns disjoint case fragments only: checks/integration-access/scenarios/fragments/task-490.2-sessions.py, task-490.2-sessions.cjs, task-490.2-authorization.py, and task-490.2-authorization.cjs. The coordinator owns lifecycle-local-bootstrap.nix, the composed Python entry point, browserSpecs mapping, and owner registration. Do not create a packet-specific NixOS test derivation.

Compose these local-auth cases after the real TASK-490.1 first-admin bootstrap in the same fresh VM/database. Reuse the created Admin and generated identity receipts; create Operator/Viewer through the real API/UI. Assert missing/mismatched CSRF receives 403 without protected-row mutation, correct CSRF persists, sign-out/revocation invalidates the selected session, and denied Operator/Viewer admin mutations leave rows unchanged. Check closed registration only under accepted owner authority; uncertain cross-environment or registration semantics remain blocked: decision. Preserve browser navigation and exact identity assertions.

Read-only sources include cf-server auth/session, auth_session, user_sessions, rbac, admin, auth_local, auth_status, routes and migration 0071; web-ui login/register/sidebar; the contract and coverage map. As-built knowledge docs do not establish D1 authority. No production edits, shared helper changes, Nix scenario composition, owner registration, CI, or documentation writes. Foundation and TASK-490.25/.26 must pass and be integrated before dispatch. The coordinator records exact combined test revision, owner command and evidence.
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
<!-- SECTION:NOTES:END -->
