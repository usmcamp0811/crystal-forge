---
id: TASK-1.7
title: Create manual testing procedure document
status: Done
assignee:
  - GLM5.1
created_date: '2026-02-04 20:19'
updated_date: '2026-03-13 01:24'
labels:
  - documentation
  - testing
milestone: m-0
dependencies: []
parent_task_id: TASK-1
ordinal: 97000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Document step-by-step manual testing procedure for deployment persistence on real NixOS system.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Document pre-deployment checks (generation list, current system)
- [x] #2 Document how to trigger deployment
- [x] #3 Document post-deployment verification steps
- [x] #4 Document reboot persistence test
- [x] #5 Document bootloader verification
- [x] #6 Include both strategies (immediate_persist and boot_only)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
VM tests limited by no internet - must test on real system
<!-- SECTION:NOTES:END -->
