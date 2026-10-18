---
id: TASK-490.23
title: 'Upgrade: real supported old-version transition and continuity scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
updated_date: '2026-10-17 17:29'
labels:
  - testing
  - integration
  - blocked-decision
dependencies:
  - TASK-490.7
parent_task_id: TASK-490
priority: high
type: task
ordinal: 535000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Current packet boundary (2026-10-18): this task owns only old/current version Python/browser fragments and immutable source fixture definitions. The coordinator owns `version-transition.nix`, pinned input wiring and Upgrade registration. Once D2 is decided, the old and current server phases run sequentially against one fresh VM/database; the in-place version/schema transition is the concrete reason Upgrade remains isolated from current-version owners. No derivation per implementation packet.

Owning check: integration-upgrade. Contract ID: upgrade.continuity. Exclusive scenario writes: checks/integration-upgrade/scenarios/continuity.{nix,py,cjs}; scenario fixtures/assets under that owner. Pinned old-version input wiring requires coordinator approval and coordinator-owned shared/flake changes; child cannot edit those paths unilaterally.

BLOCKED D2: no accepted support decision names oldest supported in-place source server release, whether old agents/builders must operate with current server and for how long, or exact pinned source tag/commit. Existing v0.3.1/v0.3.0/v0.2.x tags and crate version 0.3.0 are not support promises. Owner must decide all three. Old-source flake input/fetch must not change current application dependencies. As-built D1 is evidence, not authority; other uncertain expectations also remain blocked pending owner decision.

Scope once unblocked: build actual old package from its own pinned source/migrations. Through old API/UI create users/credentials, evaluation/build/deployment history, queued build and pending deployment. Current package starts normally on same PostgreSQL data directory and applies migrations. Existing passwords still sign in, exact history identities persist, in-flight work reaches specified states, representative new evaluate/build completes. Old agents/builders continue within the declared compatibility window. No same-version restart substitute, synthetic old schema or current-version seeded data. Non-goals: current-version recovery (490.15), retained non-integration server-regressions migration-prefix rehearsals. TASK-490 test-only scope applies.

Organization: native checks/integration-upgrade/default.nix uses { lib, inputs, pkgs, ... }, coordinator-owned explicit scenarios and normal pkgs.linkFarm aggregate of ALL registered gates. Shared helpers lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; scripts/fixtures/assets owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns default.nix/shared lib/CI/docs; child files disjoint and integration requires explicit registration. Meaningful WIP may be discoverable; only Access exists now; Upgrade default.nix only with meaningful scenario after blockers, no placeholder.

Prerequisite TASK-490.7 plus D2 decision. Candidate, NOT verified: nix build .#checks.x86_64-linux.integration-upgrade.scenarios.continuity -L; append .run for evidence; aggregate nix build .#checks.x86_64-linux.integration-upgrade -L. Completion requires explicit registration, real VM execution and exact old/current revisions, commands, outcomes and evidence; keep correct failing assertions and linked Backlog product bugs.

READINESS: foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s), accepted D2, implemented interfaces/fixtures/registration and verified commands required before To Do. No foundation hash is invented; no delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The owner decision D2 is recorded with the supported source release, compatibility window, and pinned source
- [ ] #2 Data created by the real old release survives migration with identities, credentials, and history intact
- [ ] #3 In-flight work recovers across the version transition and one new operation completes
- [ ] #4 Old agents/builders within the declared window remain usable
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native Upgrade owner-local layout, coordinator-owned registration/shared/CI/docs and input wiring. D2 remains unresolved, as does actual foundation dependency commit. Foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands plus owner support decision required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
