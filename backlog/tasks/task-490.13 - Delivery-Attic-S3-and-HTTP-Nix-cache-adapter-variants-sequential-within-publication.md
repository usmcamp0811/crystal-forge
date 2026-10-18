---
id: TASK-490.13
title: >-
  Delivery: Attic, S3, and HTTP/Nix cache adapter variants (sequential within
  publication)
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
priority: medium
type: task
ordinal: 525000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Current packet boundary (2026-10-18; supersedes prior separate-scenario paths below): this task owns sequential adapter Python/browser fragments and service fixtures under `checks/integration-delivery/scenarios/fragments/` and owner-local fixtures. Coordinator owns `delivery-lifecycle.nix`, composition and registration. Reuse actual same-run outputs; do not add one derivation per adapter or packet.

Owning check: integration-delivery. Contract ID: delivery.publication.variants. Exclusive writes: checks/integration-delivery/scenarios/publication-variants.{nix,py,cjs}. Implement sequentially Attic, S3, then HTTP/Nix. Reuse common evaluate/build/retrieve workflow from TASK-490.12; prove only each distinct adapter boundary, no lifecycle cloning. Legacy attic-metrics, attic-publication and browser steps 21-25 partly use SQL clones/private fixture rows.

Scope: Attic JWT write with scope negatives and named-cache Test; S3 access-key write/read; HTTP and Nix read-only destinations with public read and no write path. Each variant uses fresh output independently absent at destination first; wrong credentials publish nothing for write-capable adapters. Non-goal: metrics internals (non-integration). TASK-490 test-only scope applies.

Organization: native checks/integration-delivery/default.nix uses { lib, inputs, pkgs, ... }, explicit coordinator registrations, normal pkgs.linkFarm aggregate of ALL registered gates. Shared helpers lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; scripts/fixtures/assets owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns owner default.nix/shared lib/CI/docs; child integration requires explicit registration and disjoint files. Meaningful WIP can be discoverable; only Access exists now, other owners only with meaningful scenarios, no placeholders.

Prerequisites: TASK-490.12 integrated; TASK-490.7 Attic/S3 fixtures. Read cf-protocol src/cache.rs, cf-server caches.rs test-credentials and derivations/cache.rs, cf-builder Attic/S3 push, docs/knowledge/caches/*, checks/attic-metrics/README.md and checks/attic-publication/README.md. As-built D1 is evidence, not accepted authority; uncertain cases blocked pending owner decision.

Candidate, NOT verified: nix build .#checks.x86_64-linux.integration-delivery.scenarios.publication-variants -L; append .run for evidence; aggregate nix build .#checks.x86_64-linux.integration-delivery -L. Completion requires explicit registration, real VM execution and revision/commands/outcomes/evidence notes. Independent expectations; correct failing assertions and linked Backlog product bugs remain; shared gaps go to coordinator.

READINESS: actual foundation commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s), implemented interfaces/fixtures/registration and verified commands required before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Attic, S3, and HTTP/Nix each prove their distinct publication or read authentication boundary with a fresh output
- [ ] #2 Wrong credentials publish nothing for each write-capable adapter
- [ ] #3 No complete lifecycle is duplicated per adapter
- [ ] #4 Executed in real VMs with recorded evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local paths/commands and explicit coordinator registration. Foundation commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Future packet needs actual dependency commit(s) and verified commands before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
