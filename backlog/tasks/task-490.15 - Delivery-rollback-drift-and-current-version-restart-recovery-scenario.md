---
id: TASK-490.15
title: 'Delivery: rollback, drift, and current-version restart recovery scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
updated_date: '2026-10-17 17:27'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.14
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 527000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Current packet boundary (2026-10-18; supersedes prior separate-scenario paths below): this task owns rollback/drift/recovery Python/browser fragments under `checks/integration-delivery/scenarios/fragments/`. Coordinator owns `delivery-lifecycle.nix`, composition and registration. Use generations/jobs created earlier in that same fresh target VM; do not create a packet-specific derivation or depend on TASK-490.14's other VM state.

Owning check: integration-delivery. Contract ID: delivery.activation.recovery. Exclusive writes: checks/integration-delivery/scenarios/activation-recovery.{nix,py,cjs}. Legacy rollback preselection/drift steps 12o/12i-12k are mocked; restart recovery otherwise relies on Rust state-machine tests.

Scope: System detail UI rollback restores previous generation on targetA, verified through actual /run/current-system and boot entry. Out-of-band local change reports drift with actual running identity. Kill/restart server during evaluation/build and agent during deployment; bounded waits prove consistent recovery at the current version. Non-goal: version-transition recovery (Upgrade 490.23). TASK-490 test-only scope applies.

Organization: native checks/integration-delivery/default.nix has { lib, inputs, pkgs, ... }, explicit coordinator registrations and normal pkgs.linkFarm aggregate of ALL registered gates. Shared lib/integration-test through lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; scripts/fixtures/assets owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns owner default.nix/shared lib/CI/docs; child integration requires explicit registration and disjoint files. Meaningful WIP can be discoverable; only Access exists now, other owners only with meaningful scenarios, no placeholders.

Prerequisite: TASK-490.14 integrated. Read cf-server api/systems.rs rollback/rollback-generation/sync/history/agent-events, startup reset_non_terminal_derivations, cf-agent system_state.rs; docs/knowledge/deployment/* and ui/deployment-history-rendering-rules.md; SystemDetail.jsx. As-built D1 is evidence, not accepted authority; uncertain recovery states stay blocked pending owner decision.

Candidate, NOT verified: nix build .#checks.x86_64-linux.integration-delivery.scenarios.activation-recovery -L; append .run for evidence; aggregate nix build .#checks.x86_64-linux.integration-delivery -L. Completion requires coordinator registration, real VM execution and exact revision/commands/outcomes/evidence notes; independently defined expected states, retained correct failing assertions and linked Backlog product bugs; interface gaps escalate to coordinator.

READINESS: foundation dependency commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Actual dependency commit(s), implemented interfaces/fixtures/registration and verified commands required before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 UI rollback restores the previous generation on the real target
- [ ] #2 Out-of-band changes are reported as drift with the actual identity
- [ ] #3 Server and agent restarts during work recover to the specified state
- [ ] #4 Executed in real VMs with recorded evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local layout and explicit coordinator registration. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.
Scenario isolation: perform the real activation/setup generation in this scenario's own fresh targetA VM; no generation/output/runtime state from TASK-490.14 is shared. Foundation VM fails on TASK-490.25/.26; remain Backlog.
<!-- SECTION:NOTES:END -->
