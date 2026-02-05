---
id: TASK-1.6
title: Update NixOS module to expose deployment strategy option
status: In Progress
assignee: []
created_date: '2026-02-04 20:19'
updated_date: '2026-02-05 14:53'
labels:
  - nixos
  - nix
  - config
dependencies: []
parent_task_id: TASK-1
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add deployment_strategy option to crystal-forge NixOS module configuration.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Add deployment_strategy option to module
- [ ] #2 Set type to enum [immediate_persist boot_only]
- [ ] #3 Set default to immediate_persist
- [ ] #4 Add description documenting both strategies
- [ ] #5 Wire option to agent configuration
<!-- AC:END -->
