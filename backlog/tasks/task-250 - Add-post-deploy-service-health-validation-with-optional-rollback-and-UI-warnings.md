---
id: TASK-250
title: >-
  Add post-deploy service health validation with optional rollback and UI
  warnings
status: To Do
assignee: []
created_date: '2026-04-08 17:40'
labels:
  - idea
  - deployment
  - systemd
  - ui
  - reliability
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Capture a future enhancement for deployment safety: after a deployment, validate that selected critical systemd services remain active/healthy. If validation fails, support an optional rollback path to the previous generation/version. Surface validation status, failures, and rollback recommendations in the UI as visible warnings/health indicators.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A configurable list of critical systemd services can be checked automatically after deployment.
- [ ] #2 Validation results (pass/fail/degraded) are persisted and available to the UI.
- [ ] #3 UI displays post-deploy health state and clear warnings when checks fail.
- [ ] #4 A rollback strategy is defined for failed validations (manual first, optional automated rollback later).
- [ ] #5 Feature design distinguishes dry-run/simulation vs real rollback actions and includes safety guardrails.
<!-- AC:END -->
