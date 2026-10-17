---
id: TASK-490.7
title: 'Coordinator: services, builder, and target roles with a proving scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
updated_date: '2026-10-17 17:26'
labels:
  - testing
  - integration
  - coordinator
dependencies:
  - TASK-490.1
parent_task_id: TASK-490
priority: high
type: task
ordinal: 519000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Coordinator writer task. Exclusive shared writes: lib/integration-test/** and one proving scenario checks/integration-delivery/scenarios/evaluation-build-smoke.{nix,py,cjs}, with coordinator registration in checks/integration-delivery/default.nix and harness-interface coverage-map documentation. This is not a Luna packet.

Why: Delivery, Assurance, OIDC and notification scenarios need real services, builder and target roles. Add each role only with an executed proving scenario; children cannot depend on speculative interfaces.

Scope: shared roles under lib/integration-test include services { git ? false, oidc ? false, tls ? false, caches ? [], notificationReceiver ? false } using local Git HTTP derived from lib/test-flake, local OIDC, test CA/trusted TLS origin, lib/cache-test-node fixtures and notification receiver; builder { name, strategy } instantiates builderA/builderB with services.crystal-forge.build API mode and separate credentials; targetA is bootable with services.crystal-forge.client and its own key. Provide per-run nonce fresh-output fixture for assert_store_absent, agent/builder key provisioning, and offline registry from lib.crystal-forge.prefetchedPaths. Prove real commit -> evaluation -> real fresh build on builderA and concrete role use. Document actual signatures. Non-goals: implementing Delivery obligations 490.8-.15 or changing product/module semantics.

Corrected architecture: shared entry lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }. Native checks/integration-{access,delivery,assurance,upgrade}/default.nix use { lib, inputs, pkgs, ... }, explicit coordinator registrations and normal pkgs.linkFarm aggregation of ALL registered gates. Scenario scripts/fixtures/assets stay under the owning check. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Only Access exists now; add Delivery default.nix with the meaningful proving scenario, never placeholders for other owners. Meaningful WIP can be discoverable. Coordinator alone owns all owner default.nix, shared lib and CI/docs; later child integration requires explicit registration and disjoint files.

Read-only sources: lib/gitserver-test-node, lib/cache-test-node, lib/agent-test-node, lib/test-flake, checks/niks3-cache/default.nix, checks/web-ui/default.nix, modules/nixos/crystal-forge build/client/cache.encryption_key_file and docs/knowledge/builders/*. As-built D1 is evidence, not accepted authority; uncertain cases stay blocked pending owner decision. TASK-490 test-only scope applies.

Candidate, NOT verified: nix build .#checks.x86_64-linux.integration-delivery.scenarios.evaluation-build-smoke -L and same attribute with .run for evidence; nix build .#checks.x86_64-linux.integration-delivery -L aggregates all registered gates. Record actual revision, commands, VM outcomes and evidence; retain correct failing assertions with linked product bugs.

READINESS: prerequisite TASK-490.1 has no resolved actual foundation dependency commit, is blocked by two existing Rust package tests, and has no VM execution. Backlog, not ready. Before To Do, coordinator packet needs actual dependency commit(s), implemented interfaces and verified commands. No delegation or executed proving slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 lib/integration-test provides documented services, builderA/builderB and targetA role interfaces through lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }, with concrete use proven by real VM execution
- [ ] #2 checks/integration-delivery/scenarios/evaluation-build-smoke evaluates a real commit and builds a fresh output on builderA that was independently absent beforehand
- [ ] #3 Coordinator adds native checks/integration-delivery/default.nix with { lib, inputs, pkgs, ... }, explicitly registers the meaningful proving scenario and uses pkgs.linkFarm for ALL registered gates; no placeholder owners or custom discovery
- [ ] #4 Coverage map documents actual role/helper signatures and notes record actual dependency commit(s), verified commands, revision, outcomes and .run evidence
- [ ] #5 No production code or module semantics change; this is coordinator-written work and later child registration remains coordinator-owned
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: shared roles move to lib/integration-test; meaningful Delivery proving scenario is owner-local and explicitly coordinator-registered. Coordinator writer, not Luna. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or proving-slice execution is claimed.
<!-- SECTION:NOTES:END -->
