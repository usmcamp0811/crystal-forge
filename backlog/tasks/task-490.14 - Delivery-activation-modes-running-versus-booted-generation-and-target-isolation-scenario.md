---
id: TASK-490.14
title: >-
  Delivery: activation modes, running versus booted generation, and target
  isolation scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
updated_date: '2026-10-17 17:27'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.12
parent_task_id: TASK-490
priority: high
type: task
ordinal: 526000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Current packet boundary (2026-10-18; supersedes the earlier BootOnly wording and per-packet derivation implication): TASK-490.14 owns only activation Python/browser fragments under `checks/integration-delivery/scenarios/fragments/`; the coordinator owns the scenario composition and registration. Before BootOnly action, record boot selection, `/run/current-system`, and `/run/booted-system` separately. Before reboot, assert boot selection changed while each running/booted identity still equals its own pre-action value. After reboot assert target boots/runs the selected deployment. Do not infer boot selection from `/run/booted-system` or assume the two pre-action identities were equal.

Owning check: integration-delivery. Contract ID: delivery.activation.modes. Exclusive writes: checks/integration-delivery/scenarios/activation-modes.{nix,py,cjs}. Real deploy is excluded from legacy browser manifest; mocked UI and seeded deployment rows cannot prove activation.

Scope: manual, auto_latest and pinned policies on targetA. Manual changes /run/current-system only after Deploy; auto_latest follows a new evaluated commit; pinned ignores newer commits. BootOnly changes the boot entry without changing running generation until targetA reboots. BuilderB also runs a real second agent with its own key; deploying targetA leaves builderB generation/services unchanged using output independently absent in builderB store. System detail Deploy/History shows actual running versus booted identity. Non-goals: rollback/drift (490.15). TASK-490 test-only restrictions apply.

Organization: native checks/integration-delivery/default.nix uses { lib, inputs, pkgs, ... }, explicit coordinator registrations and pkgs.linkFarm aggregate of ALL registered gates. Shared lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; scripts/fixtures/assets owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns default.nix/shared lib/CI/docs; explicit child registration and disjoint scenario files required. Meaningful WIP may be discoverable; only Access exists now, other owners need meaningful scenarios, no placeholders.

Prerequisites: TASK-490.12 integrated; TASK-490.7 targetA and builderB-agent wiring. Read cf-server models/systems.rs DeploymentPolicy, cf-config config/deployment.rs DeploymentStrategy, handlers/agent/*, api/systems.rs deploy/status/generations, cf-agent deployment; docs/knowledge/deployment/*; SystemDetail.jsx/DeployGate.jsx. As-built D1 is evidence, not authority; uncertain cases blocked pending owner decision.

Candidate, NOT verified: nix build .#checks.x86_64-linux.integration-delivery.scenarios.activation-modes -L; append .run for evidence; aggregate nix build .#checks.x86_64-linux.integration-delivery -L. Completion requires coordinator registration, real VM execution and exact revision/commands/outcomes/evidence, independent expectations and retained correct failing assertions linked to Backlog product bugs. Escalate interface/authority gaps, no product changes.

READINESS: foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s), implemented helpers/fixtures/registration and verified commands required before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 manual, auto_latest, and pinned activate exactly as specified on a real target
- [ ] #2 BootOnly changes the booted generation without changing the running generation until reboot
- [ ] #3 Deploying one target leaves a second real agent unchanged
- [ ] #4 UI running/booted identities match the target's actual state
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces prior layout/commands. Coordinator owns explicit registration/shared lib/CI/docs. Foundation commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
