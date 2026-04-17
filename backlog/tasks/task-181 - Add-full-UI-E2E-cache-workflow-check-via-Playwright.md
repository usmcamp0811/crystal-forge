---
id: TASK-181
title: Add full UI E2E cache workflow check via Playwright
status: Backlog
assignee: []
created_date: '2026-03-11 03:20'
labels:
  - chore
  - testing
  - playwright
  - cache
  - web-ui
  - ci
dependencies: []
references:
  - task-42
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem:
Current web UI checks do not verify end-to-end cache management behavior through the browser. We can compile/check the UI and backend, but we are not validating that admins can create/edit/delete cache destinations in the UI and that those configurations actually drive cache push behavior.

Desired Outcome:
Add an automated Playwright-based end-to-end check that exercises cache management from the UI (create required entities, configure cache destinations, and validate runtime behavior), similar to existing server-level checks. Evaluate whether this should extend the existing server check pipeline or be a dedicated cache E2E check, then implement the chosen approach.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A Playwright E2E scenario exists that creates cache destinations via the UI and verifies successful persistence through UI/API feedback.
- [ ] #2 The scenario validates that configured caches are actually used by the system (not just created), with assertions against observable behavior/logs/API state.
- [ ] #3 The check runs in CI (or existing local verification entrypoint) with deterministic setup/teardown.
- [ ] #4 The task documents whether this check is merged into the existing server check flow or kept as a dedicated cache E2E check, with rationale.
<!-- AC:END -->
