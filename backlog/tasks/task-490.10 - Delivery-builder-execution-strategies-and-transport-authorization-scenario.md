---
id: TASK-490.10
title: 'Delivery: builder execution strategies and transport authorization scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
updated_date: '2026-10-17 17:26'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.7
parent_task_id: TASK-490
priority: high
type: task
ordinal: 522000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.build.strategies. Exclusive writes: checks/integration-delivery/scenarios/build-strategies.{nix,py,cjs}. Legacy niks3-cache proof seeds queue rows; this task must execute real fresh builds through authorized builder APIs.

Scope: builderA uses ServerDerivation, builderB uses SourceReEvaluateVerified; source delivery is ServerBundledArchive only. Both build per-run fresh outputs independently absent beforehand (assert_store_absent). Server accepts completion only from the authorized builder session; wrong key or revoked session is rejected with no job-state change. Derivation/source archive downloads require job authorization. Builds UI shows exact job/builder/output. Non-goals: scheduling/cancel (490.11), publication (490.12). TASK-490 test-only scope applies.

Organization: checks/integration-delivery/default.nix is native Snowfall { lib, inputs, pkgs, ... }, explicit coordinator-owned scenarios and normal pkgs.linkFarm aggregate of ALL registered gates. Shared helpers lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; owner-local scripts/fixtures/assets. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns owner default.nix, shared lib and CI/docs; child integration requires explicit registration and disjoint files. Meaningful WIP may be discoverable; only Access exists now; other owners require meaningful scenarios, no placeholders.

Prerequisite: TASK-490.7 integrated. Read cf-protocol src/builder.rs RemoteBuildExecutionStrategy/SourceInputDeliveryMode/NextJobRequest, cf-server handlers/api/builders.rs session/next-job/jobs routes, cf-builder; docs/knowledge/builders/verified-source-evaluator-contract.md, api/builder-job-lifecycle-api.md, security/builder-request-authentication-and-data-in-transit.md; BuildsView.jsx/BuildersView.jsx. As-built D1 is evidence, not authority; uncertain behavior remains blocked pending owner decision.

Candidate, NOT verified: nix build .#checks.x86_64-linux.integration-delivery.scenarios.build-strategies -L; append .run for evidence. Aggregate nix build .#checks.x86_64-linux.integration-delivery -L. Completion requires explicit registration, real VM execution, independent facts and exact revision/commands/outcomes/evidence notes. Correct product failures retain assertions and linked Backlog bugs; escalate interface/authority gaps, no product fixes.

READINESS: actual foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s), implemented helpers/fixtures/registration and verified commands required before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each supported strategy builds a fresh output on its own real builder
- [ ] #2 Unauthorized or revoked builder sessions are rejected with no job state change
- [ ] #3 Archive downloads enforce job authorization
- [ ] #4 Builds view shows the exact job, builder, and output identity
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local paths, shared lib/integration-test and explicit coordinator registration. Foundation commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
