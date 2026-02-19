---
id: TASK-1.4
title: Refactor activate_configuration to use strategy pattern
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
Update activate_configuration to create generation first, then activate based on configured strategy (switch vs boot).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Call create_generation before activation
- [ ] #2 Call verify_generation_created after creation
- [ ] #3 Match on self.config.strategy to determine action
- [ ] #4 Extract systemd-run logic to activate_via_systemd helper
- [ ] #5 Update logging to indicate strategy being used
<!-- AC:END -->
