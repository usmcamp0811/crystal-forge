---
id: TASK-490.1
title: >-
  Integration overhaul foundation: contract, coverage map, shared harness, and
  Access bootstrap slice
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-17 23:57'
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
Problem: children need the adopted integration contract, coverage/retirement map, native Snowfall scenario interface, and a real exercised Access slice. Goal: establish that shared test-only foundation and execute fresh server/database startup, supported first-admin local bootstrap, production-UI sign-in, and authoritative reload/read. Test-only scope from TASK-490 applies; docs/design/CrystalForge/ remains read-only.

Native architecture: checks/integration-access/default.nix uses Snowfall { lib, inputs, pkgs, ... }, coordinator-owned explicit scenarios, and pkgs.linkFarm aggregation of every registered gate. Scenario scripts/fixtures/assets live under checks/integration-access/. Shared helpers live at lib/integration-test, exposed as lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Only the meaningful Access WIP owner is registered; no placeholder owners. Coordinator owns shared helper code, check registration, flake/CI wiring and canonical docs. Child scenario writes remain disjoint and owner-local.

Implemented scenario: checks/integration-access/scenarios/lifecycle-local-bootstrap.{nix,py,cjs}. It invokes the production cf-server-drv with the normal NixOS module and records redacted generated config plus server/PostgreSQL journals on failure. Scenario actor and expected outcome: first local registrant becomes Admin; migrations apply; real browser sign-in and reload preserve the session; rejected password has no session side effect.

Current execution: TASK-491 test-only prerequisite commit af39284bdf16d56402f01a64c916f17948e43b02 was cherry-picked into this branch as a6c8922650bf7f0ff643d181422c0092cfc43242. Exact command `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L` executed the NixOS VM but FAILED the startup assertion. Evidence `/nix/store/yfc7pkz5zm8k29b82zzfhm2i8f1gn348-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. PostgreSQL starts; the generated database host is `/run/postgresql`; server exits `both host and hostaddr are missing` before migrations or HTTP. The generated [server] config has no auth_mode and `AUTH_MODE` is unset; local auth route behavior was not reached. Correct assertions remain; there is no test-only AUTH_MODE override and no production code change.

Linked Backlog product defects: TASK-490.25 (NixOS socket host formatted as a PostgreSQL URI with an empty host) and TASK-490.26 (module local auth_mode not propagated to generated runtime config). They are not implemented here. TASK-491's package build now passes, but the Access slice does not.

Packet/documentation correction: each scenario gets a fresh VM/database. Dependencies provide code/helper/fixture definitions only; no scenario may consume another run's live database, session, receipts, or successful output. Recreate required real starting evidence within the scenario. TASK-490.20 is triage only; TASK-490.21 creates/maintains plans only; TASK-490.27 finding waiver, TASK-490.28 acceptance register, and TASK-490.29 verification/closure are separate Backlog packets. No child is ready for Luna.

Verification: native Snowfall inventory and scenario/owner drv evaluation passed; `nix build .#checks.x86_64-linux.okf-knowledge --no-link -L` passed after the docs changes; no flake wiring or CI gate changed. The Access VM result is a recorded product-configuration failure, not a pass. Full Access coverage, valid foundation dependency status, and all remaining required scenario work remain incomplete. TASK-490.1 stays In Progress; no merge, deploy, packet readiness, Review or Done transition is authorized.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Contract is adopted at docs/knowledge/testing/integration-test-contract.md with implementation_status partial, adopted-versus-implemented distinction, and affected AGENTS.md/agent verification links; D1 is not authority
- [x] #2 Coverage map records accepted obligation ID, owner, scenario, independent expected result, evidence and status, plus retirement disposition for every legacy check and CI job
- [ ] #3 lib/integration-test exposes lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs } with documented concrete roles, production package selection, facts/receipts, bounded waits, fault injection, pinned rendering and failure artifacts
- [x] #4 checks/integration-access/default.nix uses { lib, inputs, pkgs, ... }, explicitly registers lifecycle-local-bootstrap and aggregates ALL registered gates with pkgs.linkFarm; scripts/fixtures/assets stay owner-local
- [x] #5 Focused checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap executes in a real VM with baseline updating disabled and failure propagation; exact command, revision, outcome and .run evidence are recorded, including linked defects where applicable
- [x] #6 Native discovery exposes meaningful Access WIP without requiring completion; no root tests/, legacyPackages.integration, mkOwner, custom discovery or placeholder Delivery/Assurance/Upgrade owners are added; legacy checks remain until cutover
- [ ] #7 Future bounded child packets record actual foundation/dependency commit(s), implemented interfaces, disjoint paths, fixtures, explicit coordinator registration and verified commands before To Do; until then all children remain Backlog
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner directories and lib/integration-test replace the previous layout and completion-only export proposal. Backlog grooming only. Foundation is blocked by two existing Rust package tests; no VM execution or Luna delegation occurred. Actual foundation dependency commit remains unresolved; do not substitute the historical base hash. All children remain Backlog and none is ready. Future packets require actual dependency commit(s) and verified commands before To Do. Uncertain D1/as-built cases require owner decision.

Historical preflight (not a current lock): branch/worktree TASK-490.1-integration-foundation at /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation was recorded from origin/dev 33de73b013b735304dfac9c69745a074a3e76ebd. Prior notes reported preserved pre-existing dev files, no main worktree, /dev/kvm present, and no live-preview requirement for this test-only task. This correction does not verify those historical environment observations.

Task status correction: foundation remains In Progress under the existing lock and worktree. Layout correction does not retire or restart this active implementation task. Dependent Luna packets remain Backlog pending verified dependency commits. Snowfall helper signature verified during evaluation: lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; } (the check passes inputs because Snowfall's library-loader inputs do not include self).

LOCK: claude-opus-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation

Run one VM test at a time until measured capacity supports a higher limit. Foundation remains In Progress, blocked pending package build; dependent children remain Backlog. No foundation readiness or verification is claimed.

Native Snowfall correction verification (2026-10-17): nix eval --json .#checks.x86_64-linux --apply builtins.attrNames succeeded: 27 attributes, exactly the prior 26 plus integration-access. nix eval --raw .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap.drvPath and the normal owner .drvPath succeeded. nix build .#checks.x86_64-linux.integration-access --dry-run --no-link succeeded and resolves the unchanged cf-server package derivation /nix/store/9pihfg4v8mp6rb4qnp148acv5cqsrb43-cf-server-0.3.0.drv (previous two failures recorded in TASK-491). Dry run does not execute a VM. No third blind retry or disabled package test was used.

Static verification succeeded via nix develop: node --check on lib/integration-test/browser/cf-browser.cjs and checks/integration-access/scenarios/lifecycle-local-bootstrap.cjs; Python ast.parse for lib/integration-test/driver.py and checks/integration-access/scenarios/lifecycle-local-bootstrap.py. Documentation validation passed (58 tests, 191 concepts, 216 Mermaid blocks); final rerun follows final edits. git diff --check passed. flake.nix exactly matches base again. Removed only overhaul-owned empty root tests directories. Production packages/modules/migrations/SQLx/UI/design and existing CI gates unchanged.

BLOCKED: TASK-491 prevents real VM execution of the foundation slice. Bootstrap/session behavior remains unverified. No children are ready; all Luna packets need recorded tested dependency commits and executable interface evidence before To Do. No Luna delegation occurred. Foundation is In Progress, not Review/Done.

Preserved corrected native Snowfall WIP in commit 683c86e03ef14b37e69c00030d6d17660398324d on TASK-490.1-integration-foundation, pushed to origin. Draft MR !335: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335 . This is NOT a tested foundation dependency commit: no VM slice executed because TASK-491 blocks the unchanged server package. Status remains In Progress, lock retained; no Review/Done or ready Luna packets. Final okf-knowledge passed after registering the new Access README in the existing OKF manifest (58 tests, 191 concepts, 216 Mermaid blocks). No full flake check was run.

TASK-491 prerequisite repaired in commit af39284bdf16d56402f01a64c916f17948e43b02; draft/normal MR !336: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/336. The normal production server package build now passes its full check phase (1794 passed, 0 failed, 682 ignored). This does NOT establish the Access VM scenario. TASK-490.1's focused scenario has not yet been rerun at this prerequisite commit; remains In Progress, blocked on executing its real VM/browser slice. TASK-491 is in Review; do not select/delegate dependent packets until coordinator integrates the prerequisite commit and verifies the slice.

Correction after TASK-491 prerequisite review: the server package build blocker is now resolved by TASK-491 commit af39284bdf16d56402f01a64c916f17948e43b02 (MR !336, status Review). The focused Access scenario still has not been executed on that commit. TASK-490.1's prior note saying it is blocked on package build is historical; current next action is coordinator to integrate/rebase the tested prerequisite and execute the real Access VM/browser slice. No owner result is implied and child packets remain Backlog.

Foundation execution update after integrating tested dependency af39284bdf16d56402f01a64c916f17948e43b02: exact command `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L` executed in the TASK-490.1 worktree. The VM ran and failed on `fresh-startup-applies-migrations`. Evidence is `/nix/store/yfc7pkz5zm8k29b82zzfhm2i8f1gn348-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`; `result.json`, `journals/controller/generated-config-summary.txt`, and `journals/controller/crystal-forge.journal` are authoritative outputs. Generated config: database.host=/run/postgresql, port=5432, user/database crystal_forge (password redacted); [server] has no auth_mode; systemd AUTH_MODE=unset. Server exits `both host and hostaddr are missing` before migrations or HTTP. The module option set to local is not reflected in generated config; runtime auth-mode response was not reached. Assertions remain, no AUTH_MODE override or production change. Linked product defects TASK-490.25 (socket URL) and TASK-490.26 (local auth propagation). TASK-491 package prerequisite pass does not complete this slice. Foundation remains In Progress and children remain Backlog.

Foundation slice execution is now verified as an actual VM run, with a failing correct startup assertion and captured evidence (not a pass): source tested `a6c8922650bf7f0ff643d181422c0092cfc43242-dirty`; exact command `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`; evidence `/nix/store/yfc7pkz5zm8k29b82zzfhm2i8f1gn348-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. Generated config summary states DB host /run/postgresql and AUTH_MODE unset; server fails `both host and hostaddr are missing` before migrations/HTTP. Correct assertions remain. Linked Backlog product defects TASK-490.25 and .26. No application changes or AUTH_MODE override.

Contract/coverage docs now explicitly state each scenario has a fresh VM and DB; dependencies share immutable code/fixture definitions only, never runtime outputs. Map adds contract IDs for finding waivers, acceptance register, POA&M plan entry and POA&M verification/closure; legacy CI manual baseline and screenshot-report jobs are classified. Assurance packets .20/.21 are narrowed and .27/.28/.29 separate their start states/authority; they all remain Backlog. The cutover task depends on product blockers and decomposed scenarios. Remaining foundation blockers: independent role implementations/proving scenario and any further harness defects; local server startup product defects prevent successful Access/browser result.

Final recorded scenario run at committed combined source revision 39a6a54b651a8b77fc3416b73195e363a782fa2e (not dirty): exact command `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`; VM exit was failure at `fresh-startup-applies-migrations`, after ~19s. Evidence `/nix/store/vcfbia521bjad2a85yca3h3n9i4m4ikf-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. Generated config summary and system journal confirm TASK-490.25 database URI failure and absent AUTH_MODE; migrations, browser, and HTTP did not run. No local auth HTTP response claimed. Correct assertion retained and linked defects remain Backlog. Final `okf-knowledge` passed (58 tests, 191 concepts, 216 Mermaid blocks); native check inventory remains 27 with only integration-access new; flake wiring unchanged. Foundation remains In Progress and not passing.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-10-17 20:38
---
Coordinator handoff note: TASK-491 package test repair is integrated in commit af39284bdf16d56402f01a64c916f17948e43b02; Access VM scenario runs and fails correctly on TASK-490.25/.26 configuration defects. Evidence and source commit are in implementation notes and coverage map. No packet is ready; this is not an owner pass.
---
<!-- COMMENTS:END -->
