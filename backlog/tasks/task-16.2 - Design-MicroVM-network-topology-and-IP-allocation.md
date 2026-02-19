---
id: TASK-16.2
title: Design MicroVM network topology and IP allocation
status: Backlog
assignee: []
created_date: '2026-02-05 15:16'
updated_date: '2026-02-19 03:39'
labels:
  - design
  - networking
  - microvm
milestone: m-1
dependencies: []
parent_task_id: TASK-16
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Design network topology for MicroVM cluster. Define IP ranges, DNS resolution, and inter-VM communication. Plan for server VM, agent VMs, and PostgreSQL VM connectivity.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Define IP address ranges for MicroVMs
- [ ] #2 Design network bridge/TAP configuration
- [ ] #3 Plan DNS resolution (hosts file vs dnsmasq)
- [ ] #4 Document VM-to-VM communication requirements
- [ ] #5 Plan SSH access from host to VMs
- [ ] #6 Create network topology diagram
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reference: microvm.nix networking options

Networking Options:
1. TAP networking - Best for VM-to-VM and host-to-VM communication
   - Requires bridge setup on host
   - Enable vhost-net for high throughput (~10 Gbps)
   - Supported by: qemu, cloud-hypervisor, firecracker, crosvm, kvmtool, stratovirt, alioth

2. User networking - Simplest, no host setup required
   - Supported by: qemu, kvmtool, vfkit
   - Good for basic internet access

3. virtiofs - For filesystem sharing
   - Supported by: qemu, cloud-hypervisor, crosvm
   - Not supported by: firecracker, kvmtool, stratovirt, alioth, vfkit

Recommended for Crystal Forge: TAP networking with static IPs for predictable VM-to-VM communication.
<!-- SECTION:NOTES:END -->
