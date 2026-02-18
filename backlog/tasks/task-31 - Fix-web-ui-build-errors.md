---
id: TASK-31
title: Fix web-ui build errors
status: In Progress
assignee: []
created_date: '2026-02-16 17:35'
updated_date: '2026-02-18 04:47'
labels:
  - ui
  - build
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
nix build .#checks.x86_64-linux.web-ui failed with existing errors in packages/web-ui/src/views/system_detail.rs (on_format_change mutability) and warnings in packages/web-ui/src/views/systems_list.rs (unused mut).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: gemini-2.5-flash on linux in /home/mcamp/code/crystal-forge/TASK-31-fix-web-ui-build-errors
<!-- SECTION:NOTES:END -->
