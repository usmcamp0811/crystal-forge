---
id: TASK-16.10
title: Optimize MicroVM resource usage and boot time
status: To Do
assignee: []
created_date: '2026-02-05 15:17'
updated_date: '2026-02-05 15:19'
labels:
  - optimization
  - microvm
  - performance
dependencies: []
parent_task_id: TASK-16
milestone: m-1
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Optimize MicroVM configurations for minimal resource usage and fast boot times. Tune memory, CPU, and disk settings.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Measure baseline resource usage
- [ ] #2 Optimize VM memory allocation
- [ ] #3 Reduce boot time (target <5s per VM)
- [ ] #4 Minimize disk usage
- [ ] #5 Document resource requirements
- [ ] #6 Create resource monitoring dashboard
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
microvm.nix optimization tips

Resource Optimization:
1. Memory: Default 512MB, tune per VM:
   - PostgreSQL: 512MB
   - Server: 1GB
   - Agent: 256MB

2. Filesystem: Choose based on priority:
   - squashfs: Smaller size, slower boot
   - erofs: Faster boot, larger size
   - Recommend erofs for dev environment

3. Hypervisor selection:
   - qemu: Most features, good performance
   - cloud-hypervisor: Faster boot, Rust-based
   - firecracker: Minimal, fastest boot
   - Recommend cloud-hypervisor for balance

4. Networking:
   - Enable vhost-net for TAP: ~10 Gbps vs ~1.5 Gbps
   - microvm.interfaces = [{ type = "tap"; vhost = true; }];

5. Boot time optimization:
   - Use erofs filesystem
   - Minimize installed packages
   - Share /nix/store from host
   - Target: <5s per VM, <10s full cluster
<!-- SECTION:NOTES:END -->
