---
id: TASK-454
title: Stabilize unrelated full Web UI harness workflows
status: Backlog
assignee: []
created_date: '2026-09-04 01:27'
updated_date: '2026-09-04 01:27'
labels:
  - web-ui
  - tests
  - playwright
dependencies: []
references:
  - TASK-440
  - checks/web-ui/tests/integration-test.js
priority: high
type: bug
ordinal: 463000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The final TASK-440 `nix flake check --keep-going -L` reproduced two authoritative Web UI failures on a clean `origin/dev` worktree. Workflow `05` cannot locate the username input during registration or login. Workflow `29k` has the setup overlay intercept POA&M controls. Workflow `30d` is already tracked by TASK-431, so this task covers only workflows `05` and `29k`. Diagnose deterministic authentication, setup state, and overlay isolation without weakening assertions.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Workflow 05 passes repeatedly in the authoritative Web UI Nix check without registration/login locator timeouts
- [ ] #2 Workflow 29k passes repeatedly without the setup overlay intercepting POA&M controls
- [ ] #3 The fixes do not weaken authentication or POA&M behavior assertions
- [ ] #4 The full authoritative Web UI check passes together with existing TASK-431 coverage
<!-- AC:END -->
