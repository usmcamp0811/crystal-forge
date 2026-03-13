---
id: TASK-1.6
title: Update NixOS module to expose deployment strategy option
status: Done
assignee:
  - Codex 5.3
created_date: '2026-02-04 20:19'
updated_date: '2026-03-13 01:24'
labels:
  - nixos
  - nix
  - config
milestone: m-0
dependencies: []
parent_task_id: TASK-1
ordinal: 100000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add deployment_strategy option to crystal-forge NixOS module configuration.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Add deployment_strategy option to module
- [x] #2 Set type to enum [immediate_persist boot_only]
- [x] #3 Set default to immediate_persist
- [x] #4 Add description documenting both strategies
- [x] #5 Wire option to agent configuration
<!-- AC:END -->
