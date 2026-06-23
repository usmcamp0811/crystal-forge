---
id: TASK-366
title: 'INFRA: Fix integration tests for NixOS 26.05 Grafana secret_key requirement'
status: Backlog
assignee: []
created_date: '2026-06-23 22:10'
labels:
  - infrastructure
  - testing
  - nixos
  - grafana
  - ci
dependencies: []
priority: high
ordinal: 318000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Integration test VM builds fail on NixOS 26.05 with:
```
Failed assertions:
- Grafana's secret key (services.grafana.settings.security.secret_key) doesn't have a default value anymore.
```

This is blocking all integration checks in CI since the nixpkgs upgrade.

## Goal

Configure Grafana secret_key properly in integration test VMs to satisfy NixOS 26.05 requirements.

## Solution Options

1. Hard-code the old default key ("SW2YcwTIb9zpOOhoPsMm") for test VMs
2. Generate a test-specific secret via file provider
3. Use a test fixture secret stored in the test configuration

Prefer option 1 for test simplicity unless there's a security concern for isolated test VMs.

## References

- https://grafana.com/docs/grafana/latest/setup-grafana/configure-grafana/#secret_key
- https://grafana.com/docs/grafana/latest/setup-grafana/configure-security/configure-database-encryption/#re-encrypt-secrets
<!-- SECTION:DESCRIPTION:END -->
