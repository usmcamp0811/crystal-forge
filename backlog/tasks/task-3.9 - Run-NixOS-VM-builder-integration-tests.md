---
id: TASK-3.9
title: Run NixOS VM builder integration tests
status: To Do
assignee: []
created_date: '2026-02-04 21:12'
labels:
  - testing
  - integration
  - nixos
dependencies: []
parent_task_id: TASK-3
milestone: m-2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Verify all builder integration tests pass after refactoring.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Run nix build .#checks.x86_64-linux.builder
- [ ] #2 Verify all tests pass
- [ ] #3 Check for performance regression
- [ ] #4 Measure build throughput before/after
- [ ] #5 Document any behavior changes
<!-- AC:END -->
