---
id: TASK-16.7
title: Create MicroVM orchestration scripts (start/stop/status)
status: Backlog
assignee: ["Codex 5.3"]
created_date: '2026-02-05 15:16'
updated_date: '2026-02-19 03:39'
labels:
  - implementation
  - tooling
  - scripts
milestone: m-1
dependencies: []
parent_task_id: TASK-16
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create shell scripts or Nix flake apps to manage MicroVM lifecycle. Support starting/stopping individual VMs or entire cluster.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Create start-all script
- [ ] #2 Create stop-all script
- [ ] #3 Create status script showing VM states and IPs
- [ ] #4 Create individual VM start/stop scripts
- [ ] #5 Add SSH helper commands
- [ ] #6 Create logs viewing helper
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Orchestration with microvm.nix

microvm.nix provides two deployment modes:

1. Declarative (systemd services):
   - Define VMs in host's flake.nix
   - Managed by systemd
   - Auto-start on boot
   - Example: microvm.vms.my-vm = { config = ...; };

2. Imperative (microvm command):
   - Manual VM management
   - Good for development
   - Commands: microvm -c vm.nix, microvm -u vm-name

For dev environment, recommend imperative mode with wrapper scripts:
- cf-vms start [vm-name]
- cf-vms stop [vm-name]
- cf-vms status
- cf-vms ssh [vm-name]
- cf-vms logs [vm-name]
<!-- SECTION:NOTES:END -->
