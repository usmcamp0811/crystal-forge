---
id: TASK-490.9
title: 'Delivery: Config Inspector values and provenance scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
updated_date: '2026-10-17 17:26'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.8
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 521000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.evaluation.config-inspector. Exclusive writes: checks/integration-delivery/scenarios/config-inspector.{nix,py,cjs} and checks/integration-delivery/fixtures/config-inspector-*. Existing mocked steps 12l-12q/13j-13q and Nix-level config-inspector/config-observer checks do not prove worker/API/UI together; this scenario needs distinct provenance fixtures and drawer bindings.

Scope: author a flake with distinguishable option values and definition files. System Config tab and flake Config Inspector show exact values/provenance for the exact revision; secret-path option is redacted in API/UI; second revision with changed value is distinguishable and historical selection stays exact. Partial inventory appears only when worker reports it. Non-goal: Nix-expression unit behavior, retained in non-integration checks. TASK-490 test-only restrictions apply.

Organization: native checks/integration-delivery/default.nix uses { lib, inputs, pkgs, ... }, explicit coordinator registration and normal pkgs.linkFarm aggregate of ALL registered gates. Helpers live at lib/integration-test through lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }. Scripts/fixtures/assets owner-local; no root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns default.nix, shared lib and CI/docs; child integration requires explicit registration and disjoint files. Meaningful WIP can be discoverable; only Access exists now, add other owners only with meaningful scenarios, no placeholders.

Prerequisite: TASK-490.8 integrated. Read cf-server bin/config-inspector-worker.rs, handlers/api/systems.rs config-inspections/config-observations/evaluated-options, flakes.rs revision outputs/module declarations; web-ui system Config tab/flake explorer; ConfigExplorer.jsx and FlakeExplorer.jsx; docs/knowledge/evaluation and Config Inspector concepts. Adapt read-only checks/config-inspector/fixture without modifying it. As-built D1 is evidence, not authority; uncertain cases blocked pending owner decision.

Candidate, NOT verified: nix build .#checks.x86_64-linux.integration-delivery.scenarios.config-inspector -L; append .run for evidence. Aggregate nix build .#checks.x86_64-linux.integration-delivery -L. Completion requires explicit registration, real VM execution and exact revision/commands/outcomes/evidence notes; independent fixture expectations, retained correct assertions and linked Backlog product bugs; interface/authority gaps escalate to coordinator.

READINESS: actual foundation commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s), implemented helpers/fixtures/registration and verified commands required before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Inspector values and provenance match authored fixture facts for the exact revision
- [ ] #2 Secret-path values are redacted in API and UI
- [ ] #3 A changed second revision is distinguishable and historical selection stays exact
- [ ] #4 Executed in real VMs with recorded evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces paths/commands with native owner-local layout and coordinator registration. Foundation dependency commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or slice execution is claimed.
<!-- SECTION:NOTES:END -->
