---
id: TASK-490.7
title: 'Coordinator: services, builder, and target roles with a proving scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
updated_date: '2026-10-19 02:12'
labels:
  - testing
  - integration
  - coordinator
dependencies:
  - TASK-490.1
references:
  - packages/default/crates/cf-server/src/handlers/api/builders.rs
  - packages/default/crates/cf-builder/src/builder/api_client.rs
  - modules/nixos/crystal-forge/default.nix
modified_files:
  - lib/integration-test/roles.nix
  - lib/integration-test/harness.nix
  - docs/knowledge/testing/integration-coverage-map.md
  - checks/integration-delivery/default.nix
  - checks/integration-delivery/scenarios/delivery-lifecycle.nix
  - checks/integration-delivery/scenarios/delivery-lifecycle.py
  - checks/integration-delivery/scenarios/fragments/source-build.py
parent_task_id: TASK-490
priority: high
type: task
ordinal: 519000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Coordinator-owned next multi-component role/proving packet. Goal: implement concrete services, separate API-only builders and bootable target interfaces in the existing shared harness, then prove their actual use in the initial Delivery lifecycle. Risk high; no production/module semantics changes. Do not reproduce retired suites or implement all .8-.15 obligations here.

Writable coordinator paths: lib/integration-test/roles.nix, harness.nix; checks/integration-delivery/default.nix and scenarios/delivery-lifecycle.nix/.py; scenarios/fragments/source-build.py; coverage-map actual interface/result sections. Native Snowfall owner with explicit scenarios and normal linkFarm over every gate. No placeholder, independent smoke gate, custom discovery or packet VM.

Planned role interfaces, not implemented claims: services {git,oidc,tls,caches,notificationReceiver}; builder {name,strategy} for builderA/B with separate credentials and real services.crystal-forge.build API mode; targetA through real client module/key. Implement/prove concrete capabilities needed by the first workflow; do not claim speculative future fixtures ready. Provide fresh key provisioning, offline prefetched inputs and per-run nonce output whose absence is independently asserted before build.

Initial proof: real authored Git input -> CF ingestion/evaluation with authoritative derivation -> API-only builderA actual build, with generated commit/job/output identities carried through receipts. Builder must have no DB access and server session/job authorization remains enforced. Target registration/heartbeat and real service use establish implemented role boundaries; publication/retrieval/deployment and extended cancel/retry/recovery remain later fragments in this SAME lifecycle, not standalone VMs.

Current source reading: production NixOS module, cf-server handlers/builder_request.rs and handlers/api/builders.rs, queries/builders.rs/build_jobs.rs, cf-builder API/execution code, cf-agent state/deployment code, cf-protocol wire types, lib/gitserver-test-node and lib/test-flake. Retired lib/cache-test-node and old check suites are historical source at5fa1f4fa, not active templates or acceptance authority. Approved product contract and task criteria define expectations.

Readiness: current controller code c254b1c54e0f40adfaa9d05b5766cfeffae9bb89 passed the normal Access aggregate with23cases and real VM. .25/.26 prerequisites remain integrated. Review status on .1, old startup failures and blanket D1 are NOT technical blockers to this packet. Services/builder/target constructors and Delivery proof remain unimplemented, so this is next coordinator implementation work, not completed coverage. Specific future policy/Upgrade decisions block only their own cases.

Verify normal Delivery aggregate and its registered lifecycle gate after implementation, recording exact source, real VM/process counts, cold output proof, results and evidence. Preserve correct failures without product repair. Independent derivations never share live DB/VM/cookies/outputs; compatible .8-.15 cases reuse earlier SAME-scenario records through receipts.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Documented services, builderA, builderB and targetA interfaces are implemented through lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }, including real fixtures, separate credentials, key provisioning and offline inputs
- [ ] #2 The coordinator composes the disjoint behavior-named source-build.py module as a real commit→evaluation→build case in delivery-lifecycle; builderA output is independently absent before build
- [ ] #3 Native Delivery owner explicitly registers delivery-lifecycle and its pkgs.linkFarm aggregate includes every registered scenario; no separate smoke gate, placeholder owner or custom discovery is required
- [ ] #4 TASK-490.8-.15 contribute disjoint fragments to the same scenario without packet-specific VM derivations; no production/module behavior changes are made
- [ ] #5 Later same-scenario cases consume earlier real outputs only through receipts; independent derivations never reuse live VM/database/output/receipt state
- [ ] #6 Actual interface signatures and tested dependency/source revision, focused command, result and evidence are recorded after the real Delivery VM run
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Next controller dependency is verified at c254b1c54e0f40adfaa9d05b5766cfeffae9bb89 (normal Access CI path23/23, new VM). This does not prove Delivery. Current services/builder/target helpers and real commit->evaluation->build proof are absent. Coordinator owns their implementation/composition/registration; .7 is the next shared-role milestone and needs no blanket D1 decision. Preserve API-only builder/no-DB boundary, separate session credentials and independent absent-before actual-output proof. Later publication/deploy/recovery extend the same lifecycle.

Remains Backlog until next coordinator execution begins; no active .7 lock or passing assertion claimed. Existing .1 Review is not a code-readiness gate under the latest owner instruction. Retired old helper/check references now identify historical source only, not a preservation obligation.

Historical: earlier packet notes pinned bba718be and reported unexecuted roles; a separate smoke derivation was rejected. The latest source-retirement/consolidation iteration retained one controller scenario and added no speculative Delivery owner.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-10-19 02:12
---
Backlog maintenance only: source-build.py is the proposed stable behavior-oriented source path, not proof of an implemented module or passing Delivery case. AC #2 retains the real commit→evaluation→build and independently absent-before-build obligations. No .cjs fragment is required for this non-browser proof. Coordinator retains the broader services/builder/target implementation context above for later work. No status change or startDelivery is authorized by this maintenance update.
---
<!-- COMMENTS:END -->
