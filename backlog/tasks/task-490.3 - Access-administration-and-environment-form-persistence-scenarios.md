---
id: TASK-490.3
title: 'Access: administration and environment form persistence scenarios'
status: Backlog
assignee: []
created_date: '2026-10-17 16:35'
updated_date: '2026-10-18 02:37'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.1
references:
  - docs/knowledge/testing/integration-coverage-map.md
  - docs/knowledge/ui/admin-console-and-login-views.md
  - docs/design/CrystalForge/components/AdminView.jsx
  - docs/design/CrystalForge/components/EnvironmentsView.jsx
modified_files:
  - checks/integration-access/scenarios/fragments/task-490.3-admin-forms.py
  - checks/integration-access/scenarios/fragments/task-490.3-admin-forms.cjs
  - >-
    checks/integration-access/scenarios/fragments/task-490.3-environment-forms.py
  - >-
    checks/integration-access/scenarios/fragments/task-490.3-environment-forms.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 515000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-access. Contract IDs: access.configuration.admin and access.configuration.resources. This packet owns disjoint case fragments only: checks/integration-access/scenarios/fragments/task-490.3-admin-forms.py/.cjs and task-490.3-environment-forms.py/.cjs. The coordinator owns lifecycle-local-bootstrap.nix, Python composition, browserSpecs mapping, and owner registration. Do not create packet-specific NixOS test derivations.

Compose after the real first-admin bootstrap in the same local-auth VM/database as TASK-490.2/.4. Create and edit users, roles, classification/retry settings, and environments through the production UI; prove authoritative DB state and reload. Cancel leaves rows unchanged. A real write failure keeps the draft and persists nothing only where approved authority defines that state; otherwise block that case as a design/decision gap. Preserve success and validation assertions.

Read-only sources: cf-server admin/environments handlers and routes; web-ui admin/environment forms; approved AdminView.jsx and EnvironmentsView.jsx for presentation only. As-built knowledge docs are not D1 authority. No production changes, shared helper, scenario composition, owner registration, CI, or docs writes. Foundation and TASK-490.25/.26 must pass before dispatch. The coordinator records the real composed VM result and evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Admin and environment form cases are composed in lifecycle-local-bootstrap; no packet-specific VM derivation is added
- [ ] #2 Each successful save is proven by DB read and reload read
- [ ] #3 Cancel leaves persisted rows unchanged
- [ ] #4 A real injected failure keeps the draft and persists nothing only where accepted requirements define that state; otherwise the exact state remains blocked
- [ ] #5 Disabling a user persists and prevents that user's sign-in
- [ ] #6 The coordinator's real VM run executes these cases and records tested revision, command, outcome and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local paths, shared lib/integration-test and explicit coordinator registration replace the prior layout; D1 as-built docs are not authority. Foundation commit unresolved; two existing Rust package tests block foundation, no VM execution. Backlog, not dispatch-ready. Actual dependency commit(s), implemented interfaces and verified commands are required before To Do. No Luna delegation or slice execution is claimed.
<!-- SECTION:NOTES:END -->
