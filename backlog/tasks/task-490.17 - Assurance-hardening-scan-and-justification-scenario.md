---
id: TASK-490.17
title: 'Assurance: hardening scan and justification scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
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
ordinal: 529000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.scanning.hardening. Exclusive writes: checks/integration-assurance/scenarios/scanning-hardening.{nix,py,cjs}. Separate from CVEs because this uses hardening worker/target service facts. Legacy hardening steps 27/28 are mocked and knownIssue.

Scope: targetA runs authored services with distinct systemd hardening settings; real scan produces expected per-service findings independently derived from authored unit settings, not CF output. Justification persists and survives reload. Rescan after real deployment changing one service updates only that service. Non-goals: STIG controls (490.19), CVEs. TASK-490 test-only scope applies.

Organization: native checks/integration-assurance/default.nix uses { lib, inputs, pkgs, ... }, explicit coordinator-owned scenarios and normal pkgs.linkFarm aggregate of ALL registered gates. Helpers lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; scripts/fixtures/assets owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns default.nix/shared lib/CI/docs; integration requires explicit registration of disjoint child files. Meaningful WIP can be discoverable; only Access exists now, other owners only with meaningful scenarios, no placeholders.

Prerequisite: TASK-490.14 integrated. Read cf-server bin/hardening-worker.rs, api/hardening.rs, systems.rs hardening/inventory/scan/justifications, modules/nixos/crystal-forge hardening options and HardeningTab.jsx. As-built D1 is evidence, not accepted authority; uncertain cases remain blocked pending owner decision.

Candidate, NOT verified: nix build .#checks.x86_64-linux.integration-assurance.scenarios.scanning-hardening -L; append .run for evidence; aggregate nix build .#checks.x86_64-linux.integration-assurance -L. Completion requires explicit registration, real VM execution and exact revision/commands/outcomes/evidence notes. Preserve correct failing assertions and linked Backlog product bugs; escalate interface/authority gaps without product fixes.

READINESS: actual foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s), implemented interfaces/fixtures/registration and verified commands required before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Hardening findings match authored per-service unit settings on a real target
- [ ] #2 Justifications persist through reload
- [ ] #3 A rescan after a real deployment updates only the changed service
- [ ] #4 Executed in real VMs with recorded evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local paths and explicit coordinator registration. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
