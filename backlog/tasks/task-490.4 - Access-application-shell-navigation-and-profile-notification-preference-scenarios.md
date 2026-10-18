---
id: TASK-490.4
title: >-
  Access: application shell navigation and profile/notification preference
  scenarios
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
  - docs/knowledge/ui/frontend-navigation-and-shared-patterns.md
  - docs/design/CrystalForge/components/Shell.jsx
  - docs/design/CrystalForge/components/ProfileView.jsx
modified_files:
  - checks/integration-access/scenarios/fragments/task-490.4-shell-navigation.py
  - >-
    checks/integration-access/scenarios/fragments/task-490.4-shell-navigation.cjs
  - checks/integration-access/scenarios/fragments/task-490.4-preferences.py
  - checks/integration-access/scenarios/fragments/task-490.4-preferences.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 516000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-access. Contract IDs: access.shell and access.configuration.preferences. This packet owns disjoint shell/navigation and preference Python/browser fragments only: `checks/integration-access/scenarios/fragments/task-490.4-shell-navigation.py/.cjs` and `task-490.4-preferences.py/.cjs`. The coordinator owns `lifecycle-local-bootstrap.nix`, Python composition, browserSpecs mapping and owner registration. No packet-specific NixOS derivations.

Compose local-auth workflows with TASK-490.1/.2/.3 in the same fresh VM/database. Verify AppShell routes, unknown-route behavior, keyboard/sidebar/narrow navigation and per-user profile/notification preferences. User A changes persist and leave User B unchanged. Compare shell presentation to the approved Shell.jsx state with a justified threshold; missing approved states remain blocked: design gap. Reuse same-run user receipts.

The base scenario passed at `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`; TASK-490.25/.26 are integrated. Remaining dependency is TASK-490.1 review/selection and approved UI states. Focused verification after composition: `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`; aggregate: `nix build .#checks.x86_64-linux.integration-access -L`. The future fragment composition has not run; keep Backlog until selected and blocked design states are resolved.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Shell and preferences cases are composed into lifecycle-local-bootstrap; no packet-specific VM derivation is added
- [ ] #2 Every accepted AppShell route is reachable and renders the expected heading; unknown routes show NotFound and protected routes redirect to /login
- [ ] #3 Keyboard, sidebar and narrow-viewport navigation follow accepted requirements; unresolved states remain blocked
- [ ] #4 An approved Shell.jsx visual comparison runs with a justified threshold and preserved reference/actual/diff images
- [ ] #5 Profile and notification preferences persist per user through reload without affecting another user
- [ ] #6 The coordinator's real VM run records tested revision, command, outcome and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces prior layout, commands and D1 authority instructions. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s), implemented helpers and verified commands are required before To Do. No Luna delegation or executed slice is claimed. Coordinator owns owner default.nix, shared lib and CI/docs; scenario files stay disjoint.
<!-- SECTION:NOTES:END -->
