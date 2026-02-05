---
id: TASK-1.7
title: Create manual testing procedure document
status: In Progress
assignee: []
created_date: '2026-02-04 20:19'
updated_date: '2026-02-05 15:04'
labels:
  - documentation
  - testing
dependencies: []
parent_task_id: TASK-1
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Document step-by-step manual testing procedure for deployment persistence on real NixOS system.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Document pre-deployment checks (generation list, current system)
- [ ] #2 Document how to trigger deployment
- [ ] #3 Document post-deployment verification steps
- [ ] #4 Document reboot persistence test
- [ ] #5 Document bootloader verification
- [ ] #6 Include both strategies (immediate_persist and boot_only)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
VM tests limited by no internet - must test on real system
<!-- SECTION:NOTES:END -->
