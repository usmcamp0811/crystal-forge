---
id: TASK-1.8
title: Test deployment persistence on real NixOS system
status: To Do
assignee: []
created_date: '2026-02-04 20:19'
labels:
  - testing
  - manual
  - deployment
dependencies:
  - TASK-1.7
parent_task_id: TASK-1
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Execute manual testing procedure on real NixOS system to verify generation creation and persistence across reboot.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Test immediate_persist strategy
- [ ] #2 Verify generation created
- [ ] #3 Verify configuration activates immediately
- [ ] #4 Reboot system and verify persistence
- [ ] #5 Test boot_only strategy
- [ ] #6 Verify configuration activates after reboot only
- [ ] #7 Verify bootloader entries updated
<!-- AC:END -->
