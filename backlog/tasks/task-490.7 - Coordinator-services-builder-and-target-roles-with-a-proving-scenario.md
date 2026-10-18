---
id: TASK-490.7
title: 'Coordinator: services, builder, and target roles with a proving scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
updated_date: '2026-10-18 03:26'
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
Coordinator-owned shared interface and single Delivery proof. Writable coordinator paths: `lib/integration-test/roles.nix`, related role-interface documentation, `checks/integration-delivery/scenarios/delivery-lifecycle.nix`, Python composition/browserSpecs, and `checks/integration-delivery/default.nix` registration. This is not a Luna child packet.

Implement the documented `services`, `builderA`, `builderB`, and `targetA` role interfaces under `lib/integration-test/`, and prove them with a real in-scenario commit→evaluation→build case in `delivery-lifecycle`. Use production packages, separate builder/agent credentials, real service fixtures, and a fresh output independently proven absent. No speculative roles or abstractions. The coordinator owns scenario composition/registration. TASK-490.8-.15 contribute disjoint Python/browser case fragments to this one fresh Delivery VM; do not create a NixOS test derivation per task. Cases use same-run commits, outputs, jobs, targets and receipts as they become available; no other derivation's live state is reused.

Current prerequisites: TASK-490.1 Access bootstrap and TASK-490.25/.26 fixes are integrated; the focused Access scenario and current owner pass at `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`. Production server package and delivery roles must be tested through the proving scenario. No unrelated module/application changes. Owner-level D1 authority still blocks uncertain domain expectations.
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
<!-- SECTION:NOTES:END -->
