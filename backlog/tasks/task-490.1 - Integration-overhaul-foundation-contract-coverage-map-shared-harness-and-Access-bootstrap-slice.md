---
id: TASK-490.1
title: >-
  Integration overhaul foundation: contract, coverage map, shared harness, and
  Access bootstrap slice
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-18 14:01'
labels:
  - testing
  - integration
dependencies: []
references:
  - /home/mcamp/code/crystal-forge/crystal-forge-integration-test-contract.md
  - packages/default/crates/cf-server/src/bin/server.rs
  - packages/default/crates/cf-server/src/handlers/api/auth_local.rs
  - packages/default/crates/cf-server/src/auth/session.rs
  - modules/nixos/crystal-forge/default.nix
  - packages/web-ui/src/routes.rs
  - checks/web-ui/default.nix
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
  - TASK-490.25
  - TASK-490.26
parent_task_id: TASK-490
priority: high
type: task
ordinal: 513000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: children need the adopted integration contract, coverage/retirement map, shared harness, and a real Access workflow. Goal: execute fresh production server/database startup, migrations, first-admin local bootstrap, embedded-UI sign-in, reload persistence, and rejected-login behavior through the normal NixOS module. Risk: medium; live preview does not apply to this test-only milestone.

Architecture: native Snowfall checks/integration-access/default.nix explicitly registers scenarios and aggregates every registered gate with pkgs.linkFarm. Shared helpers live under lib/integration-test/ and native checks call lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }. Packets contribute disjoint Python/browser fragments; coordinator owns Nix/Python composition, browserSpecs and registration. Each separate scenario gets a fresh VM/database. Later cases in the SAME scenario MAY reuse earlier real records/identities through receipts; independent derivations MUST NOT share live state. No root tests/, legacyPackages.integration, mkOwner, custom discovery or placeholder owners.

For this milestone only the controller role is implemented. Services/builder/target roles and their Delivery proving workflow remain in TASK-490.7, composed in delivery-lifecycle; no speculative Access roles or separate smoke gate. Coordinator owns shared interfaces, owner registration, contract/coverage map and affected agent/testing documentation. Non-goals: generic DSL, generated registry, coverage database, new CI service, legacy cutover, unrelated product/module fixes and design edits. The user authorized only separate TASK-490.25/.26 production repairs; current rebased commits are 52128b3a63175b712f7a83860355fdeec658a230 and 76e514644505f232e4e47f085db801913d8449ab. No other production, module, migration, SQLx, UI or design changes are authorized. Adopted requirements control expectations; D1/as-built documents are evidence, not acceptance authority.

The focused Access scenario and current integration-access owner passed at rebased source bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab8a7b372e8efe0d2dd21b6f53f0625064. Runtime evidence riky1mm0nn185wb26abw7zzshnbml0fp records 292 migrations. Focused command: `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`. This proves only bootstrap, not the full Access contract or other owners. TASK-492 is a separate first-boot Postgres Jobs role-ordering defect outside the production exception.

Packet preparation for AC #7 covers immediately reachable controller Access packets .2/.3/.4, with exact tested foundation, disjoint fragments, same-scenario placement, concrete harness interfaces and focused command. It does not require future Delivery/Assurance/Upgrade dependencies to be implemented. The user selected .2 sessions against bba718be; the coordinator owns its status transition/composition. Unresolved environment/registration authority blocks only those authorization assertions, not session proofs. .3/.4 retain exact form/design/interface blockers. Future packets remain subject to their own concrete prerequisites and selection. Keep AC #7 unchecked until the coordinator verifies this bounded packet-preparation work.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Contract is adopted at docs/knowledge/testing/integration-test-contract.md with implementation_status partial, adopted-versus-implemented distinction, and affected AGENTS.md/agent verification links; D1 is not authority
- [x] #2 Coverage map records accepted obligation ID, owner, scenario, independent expected result, evidence and status, plus retirement disposition for every legacy check and CI job
- [x] #3 lib/integration-test exposes lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs } with a documented controller role, production package selection, facts/receipts, bounded waits, fault injection, pinned rendering and failure artifacts; future services/builder/target roles and proving remain in TASK-490.7
- [x] #4 checks/integration-access/default.nix uses { lib, inputs, pkgs, ... }, explicitly registers lifecycle-local-bootstrap and aggregates all registered gates with pkgs.linkFarm; scripts/fixtures/assets stay owner-local
- [x] #5 Focused checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap executes in a real VM with baseline updating disabled and failure propagation; exact command, revision, outcome and evidence are recorded, including linked defects where applicable
- [x] #6 Native discovery exposes meaningful Access WIP without requiring completion; no root tests/, legacyPackages.integration, mkOwner, custom discovery or placeholder owners are added; legacy checks remain until cutover
- [ ] #7 Immediately reachable controller Access packets TASK-490.2/.3/.4 record tested foundation bba718be, concrete harness/receipt interfaces, disjoint Python/browser fragments, lifecycle-local-bootstrap placement, coordinator composition/registration ownership, one focused command and exact readiness/authority gaps; .2 sessions is user-selected independently of unresolved environment/registration assertions; preparation does not require every future Delivery/Assurance/Upgrade dependency
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-opus-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation

Current verification: focused Access scenario/current owner passed at bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab8a7b372e8efe0d2dd21b6f53f0625064; runtime riky1mm0nn185wb26abw7zzshnbml0fp has 292 migrations. This is bootstrap only. AC #7 is bounded to immediately reachable controller Access packet preparation and remains unchecked for coordinator review. User selected .2 sessions; status transition/composition stays with coordinator. Run one VM test at a time until measured capacity permits more. No new VM execution, delegation, merge or deployment is claimed by these spec edits.

### Historical
Initial organization/preflight was recorded from origin/dev 33de73b013b735304dfac9c69745a074a3e76ebd. Notes reported preserved pre-existing dev files, no main worktree and /dev/kvm present; those are historical observations, not newly verified environment claims. Two unchanged Rust package tests initially blocked VM execution. Shared entry requires inputs from the check because Snowfall library-loader inputs omit self.

Native Snowfall evaluation found prior 26 checks plus integration-access; owner/focused drvPath and dry-run resolved production server derivation 9pihfg4v8mp6rb4qnp148acv5cqsrb43-cf-server-0.3.0.drv without executing a VM. JavaScript syntax/Python ast.parse checks through nix develop, git diff --check and okf-knowledge passed (58 tests, 191 concepts, 216 Mermaid blocks). flake.nix matched base. Commit 683c86e03ef14b37e69c00030d6d17660398324d preserved that unexecuted WIP; draft MR !335: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335.

TASK-491 af39284bdf16d56402f01a64c916f17948e43b02 (MR !336) repaired the package tests; the production package reported 1794 passed, 0 failed, 682 ignored. After integration, the Access VM ran but failed fresh-startup-applies-migrations before migrations/HTTP. Source a6c8922650bf7f0ff643d181422c0092cfc43242-dirty produced runtime yfc7pkz5zm8k29b82zzfhm2i8f1gn348; clean 39a6a54b651a8b77fc3416b73195e363a782fa2e produced vcfbia521bjad2a85yca3h3n9i4m4ikf; fc930335878279ac426510359c039dfd8961dc13 repeated failure with 1d5178gxpdv7j4d20dddddr7ik0pq0p3. Each names a vm-test-run-cf-it-access-lifecycle-local-bootstrap-run output. Generated config/journal recorded /run/postgresql, missing server.auth_mode, AUTH_MODE=unset and `both host and hostaddr are missing`. No OIDC HTTP response or browser result was observed. Correct assertions were retained and .25/.26 linked; pending-readiness statements referred to those revisions only.

Coverage/contract reviews separated .20 triage, .21 plan, .27 waiver, .28 acceptance and .29 verification/closure. Earlier separate-run proposals prohibited all cross-run runtime sharing; current same-scenario fragments may use earlier real records via receipts. The initial controller milestone never required future .7 roles to be complete.

After separately authorized .25/.26 repairs, b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56 passed the focused command and owner aggregate (which reused its gate). Runtime xxvi4m1s6a7ajm4al64sqqb7qvhhidjz recorded all five cases, 285 migrations, socket configuration, AUTH_MODE=local, local empty setup, embedded-UI first Admin registration/sign-in/reload and wrong-password rejection with no new session/cookie. Screenshot: browser/bootstrap-sign-in/signed-in-after-reload.png. The journal also exposed separate Postgres Jobs role-ordering defect TASK-492. Exact-head ade5080cd84c7be057b8f0a9d9c490ed8b1fd3d5 repeated the pass with runtime 0aavcw4vk32c83gljg8y4mz7gi5pqrn4. Rebase onto origin/dev 5ae7feab produced current tested bba718be and rewrote repairs to 52128b3a63175b712f7a83860355fdeec658a230 and 76e514644505f232e4e47f085db801913d8449ab. Current evidence is recorded above; these historical results do not prove child fragments.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-10-17 20:38
---
Coordinator handoff note: TASK-491 package test repair is integrated in commit af39284bdf16d56402f01a64c916f17948e43b02; Access VM scenario runs and fails correctly on TASK-490.25/.26 configuration defects. Evidence and source commit are in implementation notes and coverage map. No packet is ready; this is not an owner pass.
---
<!-- COMMENTS:END -->
