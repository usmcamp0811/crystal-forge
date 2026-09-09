---
id: TASK-442
title: Make web-ui check fail when browser workflows fail
status: Backlog
assignee: []
created_date: '2026-08-30 09:36'
labels:
  - web-ui
  - testing
  - ci
dependencies: []
references:
  - checks/web-ui/default.nix
  - checks/web-ui/tests/integration-test.js
priority: high
type: bug
ordinal: 451000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The authoritative `web-ui` Nix derivation can report multiple failed Playwright workflows in `results.json` and the integration log, then print `All Mega Integration Tests Passed` and exit successfully. This creates a false-green CI result. Make the check consume the browser result status and fail whenever any selected workflow fails. Also isolate or suppress the onboarding coach consistently so it cannot intercept unrelated workflow controls.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The web-ui derivation exits non-zero when any selected browser workflow reports failure
- [ ] #2 A regression proves that failed results.json content cannot produce a successful Nix check
- [ ] #3 Unrelated workflows suppress or intentionally exercise the onboarding coach so it does not intercept controls
<!-- AC:END -->
