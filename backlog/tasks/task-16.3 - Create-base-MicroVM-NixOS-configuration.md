---
id: TASK-16.3
title: Create base MicroVM NixOS configuration
status: Backlog
assignee: ["Codex 5.3"]
created_date: '2026-02-05 15:16'
updated_date: '2026-02-19 03:39'
labels:
  - implementation
  - nix
  - microvm
milestone: m-1
dependencies: []
parent_task_id: TASK-16
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create reusable base NixOS configuration for MicroVMs. Include SSH server, minimal packages, networking setup, and Crystal Forge prerequisites.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Create base MicroVM configuration module
- [ ] #2 Enable SSH with key-based auth
- [ ] #3 Configure networking (static IPs)
- [ ] #4 Add minimal required packages
- [ ] #5 Set up user accounts
- [ ] #6 Test base VM boots and is accessible via SSH
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reference: microvm.nix base configuration

Use microvm.nix nixosModules for base configuration:

Example structure:
{
  microvm = {
    hypervisor = "qemu";  # or cloud-hypervisor, firecracker, etc.
    mem = 512;  # MB
    vcpu = 2;
    
    interfaces = [{
      type = "tap";
      id = "vm-cf-server";
      mac = "02:00:00:00:00:01";
    }];
    
    shares = [{
      source = "/nix/store";
      mountPoint = "/nix/.ro-store";
      tag = "ro-store";
      proto = "virtiofs";
    }];
  };
}

See microvm.nix examples: nix run microvm#qemu-example
<!-- SECTION:NOTES:END -->
