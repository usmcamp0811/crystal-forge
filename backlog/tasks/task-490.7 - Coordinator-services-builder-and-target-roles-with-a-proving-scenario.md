---
id: TASK-490.7
title: 'Coordinator: services, builder, and target roles with a proving scenario'
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:36'
updated_date: '2026-10-19 22:17'
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
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2964502717'
modified_files:
  - lib/integration-test/roles.nix
  - checks/integration-delivery/default.nix
  - checks/integration-delivery/README.md
  - checks/integration-delivery/fixtures/configuration.nix
  - checks/integration-delivery/scenarios/delivery-lifecycle.nix
  - checks/integration-delivery/scenarios/delivery-lifecycle.py
  - checks/integration-delivery/scenarios/fragments/source-build.py
  - docs/knowledge/testing/integration-coverage-map.md
  - .gitlab-ci.yml
parent_task_id: TASK-490
priority: high
type: task
ordinal: 519000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implements concrete shared services, separate API-only builders and a real NixOS target in the existing integration harness. The native Delivery owner explicitly registers one behavior-oriented delivery-lifecycle; no packet-specific VM, smoke gate, discovery framework or placeholder owner is permitted.

Current interfaces in roles.nix: services {} provides local smart-HTTP Git plus an authenticated SSH Nix store; builder { name, strategy ? "server_derivation" } provides the production API-only builder; target {} provides the real agent with runtime key provisioning. Controller/services/builders share immutable baseline preparation, not runtime outputs. OIDC, email, other cache fixtures and verified-source mode are not claimed implemented.

The source-build.py module proves an authored nonce-bearing Git commit, exact CF ingestion/native evaluation/attempt/derivation, authorized builder claim and actual offline Nix realization, cold-before/valid-after output, signed local publication and exact successful job/session/output identities. A real target registers and periodically reports its running generation. BuilderB establishes an independent real session after BuilderA completes, so this is not a competing-claim proof. Private credentials stay inside VMs; API-only builders have no DB section or remote controller DB connection.

Eight mandatory cases passed locally with the normal owner aggregate in the working tree based on fc0c5836. This is initial Delivery coverage, not completion of .8-.15. Later compatible ingestion, scheduling, publication, activation and recovery obligations extend this lifecycle through receipts. Independent scenarios never share live state.

The owner separately authorized TASK-495 after offline=true exposed an unsupported nix-store flag. Its independent product commit is applied without changing offline=true or weakening assertions. TASK-496 remains an observed heartbeat-classification defect; real periodic reporting does not prove the lightweight storage optimization. No other production repair, schema/design change, merge or deployment is authorized.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Documented services, builderA, builderB and targetA interfaces are implemented through lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }, including real fixtures, separate credentials, key provisioning and offline inputs
- [x] #2 The coordinator composes the disjoint behavior-named source-build.py module as a real commit→evaluation→build case in delivery-lifecycle; builderA output is independently absent before build
- [x] #3 Native Delivery owner explicitly registers delivery-lifecycle and its pkgs.linkFarm aggregate includes every registered scenario; no separate smoke gate, placeholder owner or custom discovery is required
- [x] #4 Later same-scenario cases consume earlier real outputs only through receipts; independent derivations never reuse live VM/database/output/receipt state
- [x] #5 Actual interface signatures and tested dependency/source revision, focused command, result and evidence are recorded after the real Delivery VM run
- [ ] #6 A cold input-addressed output is rejected specifically at an enforced signature boundary when unsigned or signed by an untrusted key; it remains absent/invalid, then the correctly signed equivalent succeeds with exact contents and signature evidence. Transport, access and dependencies are independently proven; no trusted-user/import/content-addressed bypass.
- [ ] #7 Controlled test-owned IPv4/IPv6 endpoints prove actual outside-boundary connection rejection with route/interface and boundary-hit evidence, while required local Git/API/SSH-store/agent traffic works. No deployed cache probe or isolation removal is permitted.
- [ ] #8 The real agent establishes exact registered state, unchanged heartbeats correlate without duplicate full states or comparison errors, a genuine observed change creates new full-state evidence, and later heartbeats correlate to that new state after the authorized TASK-496 repair.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Resume in the existing coordinator worktree /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation, branch of the same name, base dev. User explicitly selected continuation in draft MR !335. The prerequisite corrective pipeline 2961382083 passed all seven required hosted jobs at c2b77aad. Coordinator lock is recorded for this worktree.
1. Implement concrete services (local Git and required real publication destination), API-only builder {name,strategy}, and target constructors in existing roles.nix. Instantiate controller/services/builderA/builderB/targetA only once in delivery-lifecycle. No production changes or speculative future fixture interfaces.
2. Author an offline locked source fixture and a runtime nonce-bearing Git commit. Preload only immutable inputs and baseline build dependencies, never the nonce-specific result. Require actual source ingestion/native evaluation and independently verify the authorized drv/output and exact attempt identity before allowing builderA to poll.
3. Use separately generated builder/target keys and real authenticated protocols. Require builderA actual Nix realization with absent-before/present-after nonce output, successful job/identity correlation, builderB distinct session, and target registration plus real heartbeat. Native builder requires publication after build: configure a real test-owned destination as a prerequisite, without claiming complete publication coverage.
4. Explicitly register the meaningful Delivery lifecycle in a native owner aggregate. Compose semantic source-build.py sections using existing receipt/driver helpers; no browser fragment unless necessary. Later compatible delivery cases extend this lifecycle.
5. Evaluate and build the normal owner, inspect all receipts/logs and correct only test plumbing. Record truthful failures without production fixes, then run hosted owner and update existing coverage map with exact evidence, topology and remaining obligations. Keep MR draft; no merge/deploy/Done.

Owner-selected continuation at refreshed clean f6439851. User explicitly corrects this task's lifecycle: its initial role/build proof and review findings may enter Review independently of future dependent task completion. The old criterion requiring .8-.15 contributions is removed from this foundation task only; those accepted obligations remain on the children/parent gate. Coordinator owns shared roles, composition, registration and integration. First batch closes enforced signatures (inspect pinned Nix trust semantics; use untrusted receiver path in an existing VM), controlled outside-VLAN IPv4/IPv6 connectivity with positive controls and actual boundary hits, and exact heartbeat/state-lineage assertions after separate TASK-496 integration. Preserve existing cases and all offline/cold-store constraints. Then execute selected .14/.15 activation/rollback and independent Assurance work; no further product fix without authority.

Resume network finding in the existing coordinator worktree at d11de059. Add a second controlled VLAN to the same five VMs, a dual-stack HTTP probe on services, and a narrowly scoped control UID permitted only to that probe. Assert real IPv4/IPv6 route selection, same-route positive control, product-actor rejection and exact firewall counter increments. Preserve NAT isolation and verify local Git/API/SSH traffic. Compose the behavior-named fragment after the existing source/build lifecycle; run the normal Delivery aggregate and inspect its raw result. No production/UI change or signature/activation completion claim in this bounded batch.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Verified initial Delivery milestone

- Verified code: `bea0c29936c2220a8c1ba8dbfeb5750dacc18cde`.
- Hosted pipeline 2964502717 succeeded. All eight jobs passed: Access, Delivery, component, state-machine, CVE, dashboard, complexity and coverage.
- The hosted Delivery artifact records 8/8 mandatory cases passed, exact source/attempt/drv/job/builder-session/target identities, `outputAbsentBeforeBuild=true`, `outputRealized=true`, `buildCompleted=true` and `publicEgressDenied=true`.
- Hosted evidence: job17294404507 artifacts under `integration-evidence/integration-delivery/delivery-lifecycle/`. Target periodic reporting used full-state persistence; TASK-496 classification remains unresolved.
- Both normal aggregates passed locally at the same clean source: Access23/23 and Delivery8/8, exit0. Latest Delivery evidence: `/nix/store/61x9c3ii0zw0b6c4pzln0af1vcm8zhfd-vm-test-run-cf-it-delivery-delivery-lifecycle-run`; driver79.32s. Configured memory10GiB across five VMs; peak unmeasured.

## Actual interfaces and lifecycle

`services {}` supplies smart-HTTP Git and a signed/authenticated SSH Nix store. `builder {name,strategy?"server_derivation"}` supplies separate production API-only builders; `target {}` supplies the real client. Runtime keys remain in VMs; all later generated identities pass through receipts. BuilderB starts after BuilderA completes, so no competing-claim or fencing coverage is claimed. One controller/database and the same five roles serve the entire lifecycle. Independent scenarios never share live state.

## Cold preparation and history

The source generator's direct dependency fan-out measured2525->5. This is not a whole-owner recipe reduction. An opaque raw recipe closure worked warm but failed cold export on a bootstrap object before VM startup. The native baseline now prepares its complete build closure once for compatible roles. Cold preparation remains a minimization constraint, not an excuse to seed the nonce output or acquire dependencies during offline execution. Early Git, lock, signing and capacity setup failures remain historical evidence; no assertion was weakened.

TASK-495 was separately authorized, developed in its own worktree as6844d185 and applied asfc0c5836. offline=true remained unchanged. TASK-496 remains Backlog; P3/TASK-492 is also separate.

## Remaining task scope

Criteria1,2,3,5,6 are verified for the initial role/build milestone. Criterion4 concerns later functional contributions; those obligations remain unimplemented and are not marked complete. Keep this task In Progress and MR!335 draft. Further source/evaluation, scheduling, cache, activation and recovery behavior must extend appropriate semantic modules in this lifecycle, never a task-specific VM or file boundary. No merge/deploy/Done.

Preflight: existing coordinator branch/worktree is clean at f6439851, synced to origin; target origin/dev remains5ae7feab. Existing openai coordinator locks refer to this worktree. Dev has no tracked modifications; unrelated locks/session file remain untouched. The owner authorizes dependency-ready child selection for this assignment and TASK-496 only as the new product repair. No architecture reset or inventory repeat.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-10-19 02:12
---
Backlog maintenance only: source-build.py is the proposed stable behavior-oriented source path, not proof of an implemented module or passing Delivery case. AC #2 retains the real commit→evaluation→build and independently absent-before-build obligations. No .cjs fragment is required for this non-browser proof. Coordinator retains the broader services/builder/target implementation context above for later work. No status change or startDelivery is authorized by this maintenance update.
---
<!-- COMMENTS:END -->
