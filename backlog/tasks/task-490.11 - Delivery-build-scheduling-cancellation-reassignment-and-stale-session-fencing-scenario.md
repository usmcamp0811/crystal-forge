---
id: TASK-490.11
title: >-
  Delivery: build scheduling, cancellation, reassignment, and stale-session
  fencing scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
updated_date: '2026-10-17 17:27'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.10
parent_task_id: TASK-490
priority: high
type: task
ordinal: 523000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.build.scheduling. Exclusive writes: checks/integration-delivery/scenarios/build-scheduling.{nix,py,cjs} and checks/integration-delivery/fixtures/build-scheduling-*. This task provides the real failing-build fixture reused read-only by TASK-490.5; record its path and authored facts. Existing 15-15k mocked UI steps and Rust state-machine tests do not prove real execution results.

Scope: operator cancels a running build from Builds UI; builder stops and job reaches cancelled without completion. Retry requeues and real build completes. Stop/kill builderA mid-job with unit_stopped/unit_killed; lease expiry reassigns to builderB. Returning builderA's stale session cannot complete/publish (fencing). A deliberately failing derivation produces a failed job and log. Non-goals: strategy auth (490.10), notifications (490.5). TASK-490 test-only restrictions apply.

Organization: native checks/integration-delivery/default.nix has { lib, inputs, pkgs, ... }, coordinator-owned explicit scenarios and normal pkgs.linkFarm aggregate of ALL registered gates. Helpers lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; scripts/fixtures/assets owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns default.nix/shared lib/CI/docs and explicitly registers child files. Meaningful WIP can be discoverable; only Access exists now, other owners require meaningful scenarios, no placeholders.

Prerequisite: TASK-490.10 integrated. Read cf-server handlers/api/builders.rs cancel/requeue/force-cancel/prioritize/finalize-cancelled/heartbeat, queries/builders*, lease/heartbeat modules; docs/knowledge/api/builder-job-lifecycle-api.md; BuildsView.jsx. As-built D1 is evidence, not authority; uncertain requirements blocked pending owner decision.

Candidate, NOT verified: nix build .#checks.x86_64-linux.integration-delivery.scenarios.build-scheduling -L; append .run for evidence. Aggregate nix build .#checks.x86_64-linux.integration-delivery -L. Completion requires explicit registration, real VM execution and exact revision/commands/outcomes/evidence notes; independent expectations and correct failing assertions linked to Backlog product bugs. Escalate shared-interface/authority gaps, never fix product code.

READINESS: actual foundation commit unresolved; foundation blocked by two existing Rust package tests, no VM execution. Backlog, not ready. Future packet needs actual dependency commit(s), implemented helpers/fixtures/registration and verified commands before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Cancel from the UI stops real execution and records cancellation without completion
- [ ] #2 Retry produces a real completed build
- [ ] #3 A killed builder's job is reassigned and the stale session cannot complete or publish it
- [ ] #4 A failing derivation records a failed job with its log
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: owner-local scenario/fixture paths and explicit coordinator registration. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
