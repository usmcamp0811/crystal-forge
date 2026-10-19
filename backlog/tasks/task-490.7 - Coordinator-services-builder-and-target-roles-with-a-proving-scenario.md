---
id: TASK-490.7
title: 'Coordinator: services, builder, and target roles with a proving scenario'
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:36'
updated_date: '2026-10-19 15:39'
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
- [ ] #1 Documented services, builderA, builderB and targetA interfaces are implemented through lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }, including real fixtures, separate credentials, key provisioning and offline inputs
- [ ] #2 The coordinator composes the disjoint behavior-named source-build.py module as a real commit→evaluation→build case in delivery-lifecycle; builderA output is independently absent before build
- [ ] #3 Native Delivery owner explicitly registers delivery-lifecycle and its pkgs.linkFarm aggregate includes every registered scenario; no separate smoke gate, placeholder owner or custom discovery is required
- [ ] #4 TASK-490.8-.15 contribute disjoint fragments to the same scenario without packet-specific VM derivations; no production/module behavior changes are made
- [ ] #5 Later same-scenario cases consume earlier real outputs only through receipts; independent derivations never reuse live VM/database/output/receipt state
- [ ] #6 Actual interface signatures and tested dependency/source revision, focused command, result and evidence are recorded after the real Delivery VM run
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Resume in the existing coordinator worktree /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation, branch of the same name, base dev. User explicitly selected continuation in draft MR !335. The prerequisite corrective pipeline 2961382083 passed all seven required hosted jobs at c2b77aad. Coordinator lock is recorded for this worktree.
1. Implement concrete services (local Git and required real publication destination), API-only builder {name,strategy}, and target constructors in existing roles.nix. Instantiate controller/services/builderA/builderB/targetA only once in delivery-lifecycle. No production changes or speculative future fixture interfaces.
2. Author an offline locked source fixture and a runtime nonce-bearing Git commit. Preload only immutable inputs and baseline build dependencies, never the nonce-specific result. Require actual source ingestion/native evaluation and independently verify the authorized drv/output and exact attempt identity before allowing builderA to poll.
3. Use separately generated builder/target keys and real authenticated protocols. Require builderA actual Nix realization with absent-before/present-after nonce output, successful job/identity correlation, builderB distinct session, and target registration plus real heartbeat. Native builder requires publication after build: configure a real test-owned destination as a prerequisite, without claiming complete publication coverage.
4. Explicitly register the meaningful Delivery lifecycle in a native owner aggregate. Compose semantic source-build.py sections using existing receipt/driver helpers; no browser fragment unless necessary. Later compatible delivery cases extend this lifecycle.
5. Evaluate and build the normal owner, inspect all receipts/logs and correct only test plumbing. Record truthful failures without production fixes, then run hosted owner and update existing coverage map with exact evidence, topology and remaining obligations. Keep MR draft; no merge/deploy/Done.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Local execution

- Worktree/branch: TASK-490.1-integration-foundation; coordinator openai; active task lock.
- Normal command: `sh packages/ci/integration-owner-build.sh integration-delivery`.
- Latest working-tree run: 8 passed; no failed/skipped cases; source based on `fc0c58361d00466fe50e3c46d6b37e01d4db7bf8`, not a clean-revision claim.
- Evidence: `/nix/store/bys8qx6hix4kvf1yqycjbiqq2pmr1sc4-vm-test-run-cf-it-delivery-delivery-lifecycle-run`.
- Driver: 100.17 seconds. Five VMs, one controller/database; configured memory 10 GiB. Peak memory unmeasured.
- Receipts preserve exact authored SHA, native attempt UUID/number, drv, output, job, BuilderA/B IDs/sessions, target UUID/state/periodic-report IDs, output absence/realization and active public-egress rejection rules.
- Initial Git, pure-lock/package-reference, resource-budget and cache-signing setup failures were corrected in fixtures only. No assertion was weakened. A cache capacity failure was corrected by sharing immutable baseline dependencies, not increasing disk or seeding the nonce result.

## Separate product correction

TASK-495 was explicitly authorized after the real builder rejected `nix-store --offline`. Product commit `6844d185` was developed in `/home/mcamp/code/crystal-forge/TASK-495-builder-offline` and applied as `fc0c5836`. The same offline=true proof then performed a real successful realization and signed publication. TASK-496 remains Backlog; periodic target reports currently fall back to full states after a database-comparison error.

## Hosted prerequisite and remaining verification

Corrective pipelines 2961382083 (c2b77aad) and 2961428492 (e07b4bd2) passed all seven pre-Delivery required jobs. These do not prove the later Delivery source. Final clean-revision owner runs and hosted Delivery/product-fix pipeline remain to be recorded. Keep task In Progress and MR !335 draft. No merge/deploy/Done.

## History

The former planned task-numbered roles-proof fragment and proposed standalone smoke gate were superseded. The controller prerequisite was already verified; old startup failures and blanket D1 were not blockers to this implementation.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-10-19 02:12
---
Backlog maintenance only: source-build.py is the proposed stable behavior-oriented source path, not proof of an implemented module or passing Delivery case. AC #2 retains the real commit→evaluation→build and independently absent-before-build obligations. No .cjs fragment is required for this non-browser proof. Coordinator retains the broader services/builder/target implementation context above for later work. No status change or startDelivery is authorized by this maintenance update.
---
<!-- COMMENTS:END -->
