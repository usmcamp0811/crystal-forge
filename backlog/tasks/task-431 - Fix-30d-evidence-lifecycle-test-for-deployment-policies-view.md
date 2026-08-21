---
id: TASK-431
title: Fix 30d-evidence-lifecycle test for deployment-policies view
status: Backlog
assignee: []
created_date: '2026-08-21 18:00'
labels:
  - test
  - compliance
  - browser-test
dependencies: []
references:
  - checks/web-ui/tests/integration-test.js#L9756
  - checks/web-ui/coverage-manifest.json#L2014
type: bug
ordinal: 431000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The 30d-evidence-lifecycle browser test navigates to /deployment-policies and tries to click a "New policy" button, but it times out because the button doesn't exist. The test needs to be updated to match the current deployment-policies view structure. Currently profiled as "full" only and never exercised in CI. Add to ci_fast profile once the test passes.
<!-- SECTION:DESCRIPTION:END -->
