---
id: TASK-490.8
title: >-
  Delivery: Git ingestion, event deduplication, and exact evaluation identity
  scenarios
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
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
ordinal: 520000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract IDs: delivery.evaluation.ingestion, delivery.evaluation.identity. Exclusive writes: checks/integration-delivery/scenarios/evaluation-ingestion.{nix,py,cjs}, evaluation-identity.{nix,py,cjs}, and checks/integration-delivery/fixtures/evaluation-*. Legacy checks/integration seeds commits/derivations in SQL; only web-ui 13h proves real force-push, and exact commit -> drvPath -> UI identity lacks integration proof.

Scope ingestion: polling and webhooks ingest two authored commits exactly once each; replay deduplicates; force-push rewrite is detected and surfaced in Flakes UI. Identity: two distinguishable commits produce drvPaths matching independent nix eval of those exact commits on services; Evaluations list, /evaluations/:commit_id and EvalDrawer show exact identity; moving head does not rewrite history. Reuse/extend the workflow proven by TASK-490.7 without duplicating it. The coordinator owns its proving scenario; this child writes only its disjoint named files and escalates shared changes. Non-goals: Config Inspector (490.9), build obligations (490.10).

Organization: native checks/integration-delivery/default.nix uses { lib, inputs, pkgs, ... }, coordinator-owned explicit scenarios and normal pkgs.linkFarm aggregate of ALL registered gates. Helpers: lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; scripts/fixtures/assets owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns owner default.nix/shared lib/CI/docs and explicitly registers child scenarios. Meaningful WIP can be discoverable. Only Access exists now; Delivery default.nix requires meaningful scenarios from coordinator foundation work, no placeholders.

Prerequisite: TASK-490.7 actually integrated. Read cf-server handlers/api/flakes.rs, commits.rs, webhook.rs and evaluation coordinator; web-ui flakes/evaluations/EvalDrawer; approved presentation FlakesView.jsx, EvalsView.jsx, EvalDrawer.jsx; docs/knowledge/evaluation/* and api/flakes-and-evaluation-snapshot-api.md. As-built D1 is evidence, not authority; uncertain expected behavior blocked pending owner decision. TASK-490 test-only scope applies.

Candidates, NOT verified: nix build .#checks.x86_64-linux.integration-delivery.scenarios.evaluation-ingestion -L and evaluation-identity; append .run for evidence. Normal aggregate: nix build .#checks.x86_64-linux.integration-delivery -L. Completion requires explicit registration, real VM execution and exact revision/commands/outcomes/evidence notes, independent expectations and retained correct failing assertions linked to Backlog bugs.

READINESS: actual foundation commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s), implemented interfaces/fixtures/registration and verified commands are required before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each authored commit is ingested exactly once through polling and webhook; replays are deduplicated; force-push rewrite is surfaced
- [ ] #2 Recorded drvPaths equal independently computed nix eval results for two distinguishable commits
- [ ] #3 Evaluations list, detail route, and drawer show the exact identity; historical records stay exact after the head moves
- [ ] #4 Executed in real VMs with recorded evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local paths and explicit coordinator registration replace old layout. Child files are disjoint from coordinator proving scenario. Foundation commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Future packet needs actual dependency commit(s) and verified commands before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
