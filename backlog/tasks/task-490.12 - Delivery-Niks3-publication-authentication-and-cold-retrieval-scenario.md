---
id: TASK-490.12
title: 'Delivery: Niks3 publication, authentication, and cold retrieval scenario'
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
ordinal: 524000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Current packet boundary (2026-10-18; supersedes prior separate-scenario paths below): this task owns Niks3 Python/browser fragments under `checks/integration-delivery/scenarios/fragments/`. Coordinator owns the shared `delivery-lifecycle.nix`, composition and registration. Use same-run builder output receipts for publication and cold retrieval; TASK-490.13-.15 cases may continue from that state. No packet-specific derivation.

Owning check: integration-delivery. Contract ID: delivery.publication.niks3. Exclusive writes: checks/integration-delivery/scenarios/publication-niks3.{nix,py,cjs}. Niks3 (TASK-470) legacy niks3-cache/niks3-write-probe/niks3-ui checks prove pieces with seeded queue rows; replace with real evaluation -> build -> publication -> verified cold retrieval.

Scope: configure cache through Caches UI as domain setup, token write auth then mTLS. Fresh builderA output publishes to the destination bound to its ID; independent server read verifies publication. Cold targetA proves output absent first, then retrieves with signature verification. Wrong token/client certificate publishes nothing; other-environment agents receive no read credential. Non-goals: Attic/S3/HTTP/Nix (490.13), activation (490.14). Shared form behavior belongs to Access; cache credentials/Test boundaries remain Delivery. TASK-490 test-only scope applies.

Organization: checks/integration-delivery/default.nix uses native { lib, inputs, pkgs, ... }, coordinator-owned explicit scenarios and normal pkgs.linkFarm aggregate of ALL registered gates. Shared lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; scripts/fixtures/assets owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns default.nix, shared lib and CI/docs; child files are disjoint and integration requires explicit registration. Meaningful WIP can be discoverable; only Access exists now, no other owner default.nix until meaningful scenarios, no placeholders.

Prerequisites: TASK-490.10 integrated and TASK-490.7 Niks3 services fixture available. Read cf-protocol src/cache.rs CacheType/CacheReadAuth/Niks3WriteAuth, cf-server caches.rs/builders.rs publication/completion, derivations/cache.rs, cf-builder Niks3 push and cf-agent pull; docs/knowledge/caches/niks3-cache.md, checks/niks3-cache/README.md legacy assertions, lib/cache-test-node. As-built D1 is evidence, not authority; uncertain requirements blocked pending owner decision.

Candidate, NOT verified: nix build .#checks.x86_64-linux.integration-delivery.scenarios.publication-niks3 -L; append .run for evidence. Aggregate nix build .#checks.x86_64-linux.integration-delivery -L. Completion requires explicit registration, real VM execution, exact revision/commands/outcomes/evidence, independent facts and retained correct failing assertions with linked Backlog bugs. Escalate helper/authority gaps without product changes.

READINESS: actual foundation dependency commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Actual dependency commit(s), implemented interfaces/fixtures/registration and verified commands required before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A fresh output is published to Niks3 under token and mTLS write auth and verified by an independent read
- [ ] #2 A cold target retrieves it with signature verification
- [ ] #3 Wrong credentials publish nothing; out-of-environment agents get no read credential
- [ ] #4 Executed in real VMs with recorded evidence; legacy Niks3 checks are mapped as superseded
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces prior paths/commands with native owner-local layout. Coordinator owns registration/shared lib/CI/docs. Foundation commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready; actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
