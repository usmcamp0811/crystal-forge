---
id: TASK-16.3
title: Create base MicroVM NixOS configuration
status: To Do
assignee: []
created_date: '2026-02-05 15:16'
updated_date: '2026-02-05 15:19'
labels:
  - implementation
  - nix
  - microvm
dependencies: []
parent_task_id: TASK-16
milestone: m-1
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
