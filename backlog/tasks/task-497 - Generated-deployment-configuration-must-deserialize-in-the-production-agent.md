---
id: TASK-497
title: Generated deployment configuration must deserialize in the production agent
status: To Do
assignee: []
created_date: '2026-10-20 03:33'
updated_date: '2026-10-20 20:30'
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
The NixOS module's generated deployment configuration must be consumable by the production agent. A documented nondefault deployment option currently causes the module to emit polling interval "15m" while the production deserializer expects integer seconds. A real Delivery VM then repeatedly exits before registration. The owner now authorizes a bounded compatibility repair and real generated-configuration regression; preserve unrelated defaults and reject invalid values.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The production agent accepts generated configuration for a documented nondefault deployment option without a polling-interval type error.
- [ ] #2 Polling interval units and supported configuration representations are documented consistently across module and agent.
- [ ] #3 Regression coverage exercises generated configuration through the production deserializer and retains rejection of invalid values.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner authorization: current TASK-490 assignment explicitly selects TASK-497 for the smallest configuration/deserializer compatibility repair. Historical failed reproduction: /nix/store/bw95j4zs3j8wfi4dnxsnw8xrfnlw2hw6-vm-test-run-cf-it-delivery-delivery-lifecycle-run. Use a dedicated product worktree based on df11c800; no new verification package or VM solely for this boundary.
<!-- SECTION:NOTES:END -->
