---
id: TASK-490.3
title: 'Access: administration and environment form persistence scenarios'
status: Backlog
assignee: []
created_date: '2026-10-17 16:35'
updated_date: '2026-10-18 03:32'
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
Owning check: integration-access. Contract IDs: access.configuration.admin and access.configuration.resources. This packet owns disjoint Python/browser fragments only: `checks/integration-access/scenarios/fragments/task-490.3-admin-forms.py/.cjs` and `task-490.3-environment-forms.py/.cjs`. The coordinator owns `lifecycle-local-bootstrap.nix`, Python composition, browserSpecs mapping, and owner registration. No packet-specific NixOS derivations.

Compose after the real first-admin bootstrap in the same local-auth VM/database as TASK-490.2/.4. Create/edit users, roles, classification/retry settings and environments through the production UI; prove DB state and reload. Cancel leaves rows unchanged. A real write failure preserves the draft only where accepted authority defines that state; otherwise block it as a design/decision gap. Preserve success/validation assertions.

The base scenario passed at `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`; TASK-490.25/.26 are integrated. Remaining dependency is TASK-490.1 review/selection and accepted design states for failure/cancel behavior. Controller/browser interfaces and fragment paths are concrete. Focused verification after composition: `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`; aggregate: `nix build .#checks.x86_64-linux.integration-access -L`. Neither includes these fragments yet; keep Backlog until selected and authority cases are resolved.
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
