---
id: TASK-1
title: 'BUG: Agent Deployments Don''t Persist Across Reboots'
status: Backlog
assignee: ["Codex 5.3"]
created_date: '2026-02-04 20:15'
updated_date: '2026-02-19 03:38'
labels:
  - bug
  - deployment
  - agent
  - nixos
milestone: m-0
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Agent deployments use switch-to-configuration switch without creating NixOS generations, causing deployments to be lost on reboot. Root cause: packages/default/src/deployment/agent.rs:462
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Add DeploymentStrategy enum to config/deployment.rs
- [ ] #2 Add strategy field to DeploymentConfig with default
- [ ] #3 Update activate_configuration to create generation first
- [ ] #4 Extract create_generation method
- [ ] #5 Extract activate_via_systemd method with action parameter
- [ ] #6 Add verify_generation_created method
- [ ] #7 Update configuration documentation
- [ ] #8 Add unit tests for generation creation logic
- [ ] #9 Create manual testing procedure document
- [ ] #10 Test on real NixOS system (not VM)
- [ ] #11 Verify generation persists across reboot
- [ ] #12 Update NixOS module to expose strategy option
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Solution: Make deployment strategy configurable, default to immediate_persist (create generation + switch). Testing must be manual on real NixOS system due to VM internet limitations.
<!-- SECTION:NOTES:END -->
