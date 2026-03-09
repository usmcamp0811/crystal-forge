---
id: TASK-123.4
title: Harden evaluator fallback to use strict mode for core policy
status: In Progress
assignee: []
created_date: '2026-03-09 22:47'
updated_date: '2026-03-09 22:47'
labels:
  - backend
  - evaluator
  - bug
  - policies
  - security
milestone: m-13
dependencies: []
parent_task_id: TASK-123
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

When DB policy loading fails, or when parsing yields no valid policies, the evaluator falls back to `RequireCrystalForgeAgent { strict: false }`.

Given this MR is explicitly trying to harden core deployment-policy safety and enforce a canonical always-enabled core policy, that fallback feels weaker than the intended posture.

Using `strict: false` means systems without the agent package installed can still pass evaluation in fallback scenarios, which contradicts the "always enforce core policy" safety model.

## Goal

Change the evaluator fallback to use `RequireCrystalForgeAgent { strict: true }`, ensuring that even in error/fallback scenarios, the core security policy is enforced strictly.

## Non-Goals

- Changing the normal (non-fallback) policy loading path
- Adding retry logic for DB failures
- Changing the RequireCrystalForgeAgent policy implementation itself
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Evaluator fallback creates RequireCrystalForgeAgent { strict: true } instead of strict: false
- [ ] #2 Add code comment explaining why strict mode is used even in fallback
- [ ] #3 Backend test verifies fallback policy is strict
- [ ] #4 Manual test: Force DB failure, verify evaluator uses strict fallback
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-agent on gray in ~/code/crystal-forge/TASK-123-deployment-policies-crud
<!-- SECTION:NOTES:END -->
