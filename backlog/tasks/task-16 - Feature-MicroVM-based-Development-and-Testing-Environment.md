---
id: TASK-16
title: 'Feature: MicroVM-based Development and Testing Environment'
status: To Do
assignee: []
created_date: '2026-02-05 15:16'
updated_date: '2026-02-05 15:19'
labels:
  - feature
  - microvm
  - development
  - testing
dependencies: []
priority: high
milestone: m-1
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace process-compose with MicroVM-based environment for dev/test. Each component (server, agent, PostgreSQL) runs in isolated MicroVMs with networking. Enables realistic testing without interfering with host CF installation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 MicroVM solution selected and documented
- [ ] #2 Network topology designed with IP allocation
- [ ] #3 Base MicroVM configuration created
- [ ] #4 PostgreSQL VM implemented and tested
- [ ] #5 Server VM implemented and tested
- [ ] #6 Agent VM(s) implemented and tested
- [ ] #7 Orchestration scripts created (start/stop/status)
- [ ] #8 Integration tests passing
- [ ] #9 Developer documentation complete
- [ ] #10 Resource usage optimized
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Project Reference: microvm.nix

GitHub: https://github.com/astro/microvm.nix
Handbook: https://astro.github.io/microvm.nix/

This project provides the foundation for our MicroVM-based dev environment. It offers:
- Multiple hypervisor options (recommend qemu or cloud-hypervisor)
- NixOS integration via flake modules
- TAP networking for VM-to-VM communication
- Fast boot times and efficient resource usage
- Can run as systemd services or standalone

Installation:
nix registry add microvm github:astro/microvm.nix

Quick start:
nix flake init -t microvm
nix run microvm#qemu-example

All subtasks should leverage microvm.nix rather than building from scratch.
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 All VMs boot successfully and communicate
- [ ] #2 Can SSH to all VMs from host
- [ ] #3 Server connects to PostgreSQL VM
- [ ] #4 Agent(s) register with server
- [ ] #5 Can test deployments end-to-end
- [ ] #6 No interference with host CF installation
- [ ] #7 Integration tests pass reliably
- [ ] #8 Documentation allows new developers to use environment
- [ ] #9 Boot time <10s for full cluster
- [ ] #10 Resource usage <4GB RAM total
<!-- DOD:END -->
