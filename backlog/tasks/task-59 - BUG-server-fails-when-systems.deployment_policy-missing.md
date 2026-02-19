---
id: TASK-59
title: 'BUG: server fails when systems[].deployment_policy missing'
status: To Do
assignee: []
created_date: '2026-02-19 03:04'
labels: []
dependencies: []
priority: high
milestone: m-0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
Server startup fails in deployed/dev environments when config contains systems entries without deployment_policy, producing: missing configuration field "systems[0]deployment_policy".

Goal
Allow startup with legacy configs while preserving explicit deployment policy behavior.

Non-Goals
- Do not change deployment execution semantics beyond defaulting missing values.
- Do not refactor unrelated config parsing paths.

Verification Plan
- Reproduce failure with fixture config lacking deployment_policy.
- Add tests proving missing field deserializes to safe default.
- Run nix-based check/test/build commands for server package.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Server starts when systems entries omit deployment_policy
- [ ] #2 Missing deployment_policy defaults to a documented safe value
- [ ] #3 Existing configs with explicit deployment_policy still parse unchanged
- [ ] #4 Unit tests cover both missing and explicit deployment_policy cases
- [ ] #5 nix build .#packages.x86_64-linux.server succeeds with fix
<!-- AC:END -->
