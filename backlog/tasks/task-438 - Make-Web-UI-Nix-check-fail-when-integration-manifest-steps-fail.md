---
id: TASK-438
title: Make Web UI Nix check fail when integration manifest steps fail
status: Backlog
assignee: []
created_date: '2026-08-26 01:33'
labels:
  - web-ui
  - testing
  - nix
dependencies: []
references:
  - TASK-433.5
modified_files:
  - checks/web-ui/default.nix
  - checks/web-ui/tests/integration-test.js
priority: high
type: bug
ordinal: 447000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The authoritative `checks.x86_64-linux.web-ui` derivation can exit successfully even when `integration-test.js` reports failed manifest steps. During TASK-433.5 verification, `nix flake check --keep-going -L` reported all checks passed while the Web UI report captured 63/104 screenshots and listed numerous failed steps. This makes derivation success insufficient proof and can mask regressions.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The Web UI Nix check exits non-zero when any required selected integration step fails
- [ ] #2 The check preserves and reports the failed step names and reasons
- [ ] #3 A regression test proves a deliberately failing selected step causes the derivation to fail
<!-- AC:END -->
