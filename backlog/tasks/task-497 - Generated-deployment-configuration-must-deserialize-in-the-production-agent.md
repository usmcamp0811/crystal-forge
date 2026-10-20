---
id: TASK-497
title: Generated deployment configuration must deserialize in the production agent
status: Backlog
assignee: []
created_date: '2026-10-20 03:33'
labels: []
dependencies: []
references:
  - modules/nixos/crystal-forge/default.nix
  - packages/default/crates/cf-config/src/config/deployment.rs
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
priority: high
type: bug
ordinal: 548000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A real Delivery VM could not register its agent after enabling a documented nondefault deployment option. The NixOS module emitted deployment.deployment_poll_interval as the default string "15m" while the production agent expected an integer. The agent repeatedly exited with a configuration parse error. This affects users who cause the deployment TOML block to be generated, even when they do not change the polling interval. Observed with post_agent_start_deployment_delay=0 in the connected TASK-490 increment based on 3db47c89. Failed evidence: /nix/store/bw95j4zs3j8wfi4dnxsnw8xrfnlw2hw6-vm-test-run-cf-it-delivery-delivery-lifecycle-run. No repair is authorized by TASK-496. The selected default deployment path can continue without a nondefault delay.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The production agent accepts generated configuration for a documented nondefault deployment option without a polling-interval type error.
- [ ] #2 Polling interval units and supported configuration representations are documented consistently across module and agent.
- [ ] #3 Regression coverage exercises generated configuration through the production deserializer and retains rejection of invalid values.
<!-- AC:END -->
