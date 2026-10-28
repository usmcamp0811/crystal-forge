---
id: TASK-485
title: Restore full Web UI TASK-440 captures blocked by coach overlays
status: Backlog
assignee: []
created_date: '2026-10-13 18:01'
labels: []
dependencies: []
references:
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/default.nix
  - packages/web-ui/src/components/onboarding/coach_panel.rs
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17127883247'
priority: medium
type: bug
ordinal: 507000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Optional job17127883247 at269b58ad/pipeline2940782379 records only2 of16 TASK-440 Dioxus semantic contracts. The seven remaining canonical dark workflows abort on coach-overlap guards before semantic results/light captures are appended. Source-equal harness/coach components exist on current dev ec71ca0c, but no exact-current-dev144-workflow passing pipeline was available, so this is not labeled a proven baseline-only regression. Retain all overlap/completeness assertions and baseline policy; investigate disposable browser fixtures without expanding TASK-470 cache work.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Identify why minimized coach callouts overlap canonical captures at both viewport sizes
- [ ] #2 Restore all16 canonical semantic results without hiding meaningful overlays or lowering thresholds
- [ ] #3 Run the current full workflow suite and report actual result counts and baseline comparison
<!-- AC:END -->
