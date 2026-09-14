---
id: TASK-461
title: Isolate broad Web UI workflow state
status: Backlog
assignee: []
created_date: '2026-09-09 02:45'
labels:
  - web-ui
  - test-isolation
dependencies: []
references:
  - TASK-440
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/323'
modified_files:
  - checks/web-ui/default.nix
  - checks/web-ui/tests/integration-test.js
priority: high
type: bug
ordinal: 470000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The broad authoritative `web-ui` check leaks fixture and UI state across workflows. A 2026-09-09 TASK-440 correction run passed `12l-task440-config-lifecycle` alone and in the focused 12l/12m/12n group, but the broad suite later reported no selected V2 snapshot after both Config Inspector jobs succeeded. The same broad run showed the Setup Coach intercepting unrelated cache and policy clicks, stale/missing cache/scanning/build fixtures, and authorization-state assertions failing. Make broad workflows independent so earlier workflows cannot alter later fixture authority, authentication, overlays, or Config Inspector selection state.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The broad `web-ui` check passes with each workflow starting from its declared fixture and authentication state.
- [ ] #2 The Setup Coach does not intercept controls in workflows that do not test onboarding.
- [ ] #3 `12l-task440-config-lifecycle` selects its persisted V2 snapshot in both focused and broad runs.
- [ ] #4 Authorization-role workflows do not leak role state into later workflows.
<!-- AC:END -->
