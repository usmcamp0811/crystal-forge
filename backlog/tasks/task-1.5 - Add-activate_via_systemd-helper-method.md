---
id: TASK-1.5
title: Add activate_via_systemd helper method
status: Done
assignee: ["Codex 5.3"]
created_date: '2026-02-04 20:19'
updated_date: '2026-02-05 14:53'
labels:
  - deployment
  - refactoring
  - rust
dependencies: []
parent_task_id: TASK-1
milestone: m-0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extract systemd-run execution logic into separate method that takes action parameter (switch or boot).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Create activate_via_systemd method with action: &str parameter
- [ ] #2 Move systemd-run command construction to new method
- [ ] #3 Reuse existing error handling logic
- [ ] #4 Add debug logging with shell_join for command visibility
<!-- AC:END -->
