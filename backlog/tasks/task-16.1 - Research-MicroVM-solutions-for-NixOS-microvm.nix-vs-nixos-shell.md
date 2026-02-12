---
id: TASK-16.1
title: Research MicroVM solutions for NixOS (microvm.nix vs nixos-shell)
status: To Do
assignee: []
created_date: '2026-02-05 15:16'
updated_date: '2026-02-05 15:19'
labels:
  - research
  - microvm
  - nix
dependencies: []
parent_task_id: TASK-16
milestone: m-1
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Evaluate microvm.nix and other MicroVM solutions for NixOS. Compare features, networking capabilities, resource usage, and ease of use. Document findings and recommend solution.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Research microvm.nix capabilities and limitations
- [ ] #2 Research nixos-shell and alternatives
- [ ] #3 Compare networking options (TAP, macvtap, user networking)
- [ ] #4 Document resource requirements (memory, CPU)
- [ ] #5 Create comparison matrix with pros/cons
- [ ] #6 Recommend solution with justification
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reference: microvm.nix project (https://github.com/astro/microvm.nix)

This Nix Flake provides a framework for building and running NixOS MicroVMs with multiple hypervisor options.

Key Features:
- 8 hypervisor options (qemu, cloud-hypervisor, firecracker, crosvm, kvmtool, stratovirt, alioth, vfkit)
- Read-only root disk with /nix/store (squashfs or erofs)
- Supports TAP networking, user networking, 9p/virtiofs shares
- Can run as Nix packages or systemd services
- Default 512MB RAM (adjustable)
- Fast boot times and high performance via virtio

Installation:
nix registry add microvm github:astro/microvm.nix
nix flake init -t microvm

Examples available at: nix run microvm#qemu-example

This is the RECOMMENDED solution for Crystal Forge MicroVM implementation.
<!-- SECTION:NOTES:END -->
