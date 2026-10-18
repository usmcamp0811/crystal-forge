---
id: TASK-490.7
title: 'Coordinator: services, builder, and target roles with a proving scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
updated_date: '2026-10-18 13:57'
labels:
  - testing
  - integration
  - coordinator
dependencies:
  - TASK-490.1
modified_files:
  - lib/integration-test/roles.nix
  - lib/integration-test/harness.nix
  - docs/knowledge/testing/integration-coverage-map.md
  - checks/integration-delivery/default.nix
  - checks/integration-delivery/scenarios/delivery-lifecycle.nix
  - checks/integration-delivery/scenarios/delivery-lifecycle.py
  - checks/integration-delivery/scenarios/fragments/task-490.7-roles-proof.py
  - checks/integration-delivery/scenarios/fragments/task-490.7-roles-proof.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 519000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Coordinator-owned role implementation and proving packet. Goal: give Delivery, Assurance, OIDC and notifications concrete services/builder/target interfaces, each justified by real execution. Risk: high (shared VM interfaces). Non-goals: implementing the Delivery .8-.15 obligations or changing production/module semantics.

Writable paths (one coordinator list):
- lib/integration-test/roles.nix
- lib/integration-test/harness.nix
- docs/knowledge/testing/integration-coverage-map.md (actual role/helper interface documentation)
- checks/integration-delivery/default.nix
- checks/integration-delivery/scenarios/delivery-lifecycle.nix
- checks/integration-delivery/scenarios/delivery-lifecycle.py (composition)
- checks/integration-delivery/scenarios/fragments/task-490.7-roles-proof.py
- checks/integration-delivery/scenarios/fragments/task-490.7-roles-proof.cjs

Composition target: checks/integration-delivery/scenarios/delivery-lifecycle.nix. Implement roles and compose the disjoint roles-proof fragments as its initial real commit→evaluation→build case. This is the existing coordinator task, not a child receiving a new VM derivation. Native owner default.nix uses { lib, inputs, pkgs, ... }, explicit scenario registration and pkgs.linkFarm over every registered gate. Shared entry is lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }. No placeholders, custom discovery, root tests/, legacyPackages.integration or mkOwner.

Implement services { git ? false, oidc ? false, tls ? false, caches ? [], notificationReceiver ? false }: local Git HTTP from lib/test-flake, local OIDC, test CA/trusted TLS origin, cache fixtures and notification receiver. Implement builder { name, strategy } as builderA/builderB with services.crystal-forge.build API mode and separate credentials. targetA is bootable, uses services.crystal-forge.client and its own key. Provide key provisioning, per-run nonce fresh-output fixture for assert_store_absent and offline registry using lib.crystal-forge.prefetchedPaths. Document actual signatures and prove concrete role use; do not add speculative roles. Read-only source references: lib/gitserver-test-node, lib/cache-test-node, lib/agent-test-node, lib/test-flake, checks/niks3-cache/default.nix, checks/web-ui/default.nix, NixOS build/client/cache.encryption_key_file options and docs/knowledge/builders/*. As-built documentation is evidence, not accepted authority for uncertain behavior.

TASK-490.8-.15 contribute disjoint fragments to this single fresh Delivery VM/database. Later cases MAY consume earlier real commits, jobs, outputs and target records through receipts. Independent scenario derivations MUST NOT share live VM/database/output/receipt state. Prove the build output independently absent before the real builderA build; production packages and real service fixtures remain required.

Dependency/readiness: TASK-490.1 bootstrap prerequisite passed at bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab8a7b372e8efe0d2dd21b6f53f0625064, with .25/.26 integrated; recorded runtime riky1mm0nn185wb26abw7zzshnbml0fp has 292 migrations. That is not a Delivery proof. This packet remains unimplemented/unexecuted and awaits coordinator selection and concrete role configuration, not resolution of the historical foundation failures.

Focused verification after implementation: `nix build .#checks.x86_64-linux.integration-delivery.scenarios.delivery-lifecycle -L`. Record exact tested dependency/source revisions, command, exit/result and evidence paths. Correct failures retain assertions and linked product defects; interface/authority conflicts go to coordinator. Normal owner registration includes the proof without requiring a separate smoke gate.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Documented services, builderA, builderB and targetA interfaces are implemented through lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }, including real fixtures, separate credentials, key provisioning and offline inputs
- [ ] #2 The coordinator composes the disjoint task-490.7-roles-proof.py/.cjs fragments as a real commit→evaluation→build case in delivery-lifecycle; builderA output is independently absent before build
- [ ] #3 Native Delivery owner explicitly registers delivery-lifecycle and its pkgs.linkFarm aggregate includes every registered scenario; no separate smoke gate, placeholder owner or custom discovery is required
- [ ] #4 TASK-490.8-.15 contribute disjoint fragments to the same scenario without packet-specific VM derivations; no production/module behavior changes are made
- [ ] #5 Later same-scenario cases consume earlier real outputs only through receipts; independent derivations never reuse live VM/database/output/receipt state
- [ ] #6 Actual interface signatures and tested dependency/source revision, focused command, result and evidence are recorded after the real Delivery VM run
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: bba718be is the tested controller foundation. Roles and Delivery proof remain unimplemented/unexecuted; coordinator owns selection and integration.

### Historical
2026-10-17 organization review recorded unresolved foundation/package prerequisites and no VM execution at that time. A standalone smoke scenario was a former proposal. The rebased Access prerequisite and current Access aggregate later passed at bba718befbc183863781656df2df272e5f735d5a; this does not prove Delivery roles.
<!-- SECTION:NOTES:END -->
