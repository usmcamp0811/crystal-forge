---
id: TASK-490.7
title: 'Coordinator: services, builder, and target roles with a proving scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
updated_date: '2026-10-18 13:47'
labels:
  - testing
  - integration
  - coordinator
dependencies:
  - TASK-490.1
modified_files:
  - lib/integration-test/roles.nix
  - lib/integration-test/harness.nix
  - docs/knowledge/testing/integration-coverage-map.md
  - checks/integration-delivery/scenarios/delivery-lifecycle.nix
  - checks/integration-delivery/scenarios/delivery-lifecycle.py
  - checks/integration-delivery/default.nix
parent_task_id: TASK-490
priority: high
type: task
ordinal: 519000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Coordinator-owned shared-interface and Delivery proving task. Writable coordinator paths include `lib/integration-test/roles.nix`, role-interface documentation, `checks/integration-delivery/scenarios/delivery-lifecycle.nix`, its Python composition/browserSpecs, and `checks/integration-delivery/default.nix` registration.

Implement the documented `services`, `builderA`, `builderB`, and `targetA` interfaces. Prove them with a real commit→evaluation→build case in the registered `delivery-lifecycle` scenario. Use production packages, separate credentials, real service fixtures, and a fresh output that was absent before build. Do not add speculative roles. TASK-490.8-.15 contribute disjoint case fragments to that same fresh Delivery VM; no NixOS derivation per packet. Later cases use same-run commits, outputs, jobs, targets and receipts. Independent derivations never share runtime state.

The TASK-490.1 focused Access scenario and owner now pass after rebase at `bba718befbc183863781656df2df272e5f735d5a`. This is a real prerequisite, but roles and Delivery proof remain unimplemented. Focused command after implementation: `nix build .#checks.x86_64-linux.integration-delivery.scenarios.delivery-lifecycle -L`; aggregate: `nix build .#checks.x86_64-linux.integration-delivery -L`. This is the existing coordinator task, not a new packet.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The documented services, builderA, builderB and targetA interfaces are implemented through lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }
- [ ] #2 The coordinator's delivery-lifecycle scenario executes a real commit→evaluation→build proof with builderA and a fresh output absent before the build
- [ ] #3 The normal Delivery owner explicitly registers delivery-lifecycle and pkgs.linkFarm includes it
- [ ] #4 TASK-490.8-.15 contribute disjoint case fragments to the same scenario; no packet-specific VM derivations are added
- [ ] #5 Same-scenario later cases consume only earlier real outputs through receipts; independent NixOS runs never reuse runtime state
- [ ] #6 The coordinator records revision, exact focused and aggregate commands, result and evidence path
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: shared roles move to lib/integration-test; meaningful Delivery proving scenario is owner-local and explicitly coordinator-registered. Coordinator writer, not Luna. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No delegation or proving-slice execution is claimed.

The TASK-490.1 prerequisite passed again after rebase at `bba718befbc183863781656df2df272e5f735d5a`, with the current `integration-access` aggregate passing. This is not the Delivery roles/proving scenario; TASK-490.7 remains Backlog until its interfaces and delivery-lifecycle proof are implemented and executed.
<!-- SECTION:NOTES:END -->
