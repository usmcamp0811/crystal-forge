---
id: TASK-434
title: Fail web-ui Nix check when browser steps report failures
status: Backlog
assignee: []
created_date: '2026-08-24 14:17'
labels:
  - web-ui
  - testing
  - nix
  - playwright
dependencies: []
references:
  - TASK-410
  - checks/web-ui/default.nix
  - checks/web-ui/tests/integration-test.js
modified_files:
  - checks/web-ui/default.nix
  - checks/web-ui/tests/integration-test.js
priority: high
type: bug
ordinal: 443000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The authoritative `checks.x86_64-linux.web-ui` derivation can exit successfully while `screenshots/results.json` contains browser steps with `"ok": false`. This lets failed semantic assertions appear as a passing Nix check and weakens UI verification.

## Desired outcome
Make the web-ui Nix check fail after preserving screenshots/reports whenever any executed browser step reports failure, while retaining useful artifacts for diagnosis.

## Context
Discovered during TASK-410 verification: `nix build .#checks.x86_64-linux.web-ui` exited 0 while the generated full-run results contained multiple failed steps. Focused reruns were needed to establish dashboard behavior independently.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The web-ui Nix check exits nonzero when any executed browser step records ok false
- [ ] #2 Failure screenshots results.json and visual reports remain available for diagnosis
- [ ] #3 The web-ui Nix check exits zero when every selected browser step passes
- [ ] #4 Focused CF_UI_TEST_STEPS runs and normal full-profile runs use the same failure propagation
- [ ] #5 Automated regression coverage proves both failing-step and all-passing outcomes
<!-- AC:END -->
