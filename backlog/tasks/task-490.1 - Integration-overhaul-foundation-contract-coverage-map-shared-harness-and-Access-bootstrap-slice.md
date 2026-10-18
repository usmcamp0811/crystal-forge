---
id: TASK-490.1
title: >-
  Integration overhaul foundation: contract, coverage map, shared harness, and
  Access bootstrap slice
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-18 03:52'
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
Problem: children need the adopted integration contract, coverage/retirement map, shared harness and a real Access workflow. Goal: establish those foundations and execute fresh server/database startup, migrations, local first-admin bootstrap, production-UI sign-in, reload persistence and wrong-password rejection through the normal NixOS module.

Architecture: native Snowfall `checks/integration-access/default.nix` explicitly registers scenarios and aggregates every registered gate with `pkgs.linkFarm`. Shared helpers live under `lib/integration-test/` and are called as `lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }`. Packet files are fragments; coordinator owns Nix scenario composition and registration. No root tests/, `legacyPackages.integration`, `mkOwner`, custom discovery, or placeholder owners. Each separately built scenario has a fresh VM/database; same-scenario cases may use real earlier records/identities through receipts; no cross-derivation state.

Harness scope for this milestone: production `cf-server-drv` with embedded UI through the normal module; currently implemented role is controller with browser option. Services/builder/target role interfaces and their Delivery proving workflow remain in coordinator-owned TASK-490.7; do not add speculative roles/helpers to Access. Contract, coverage map, docs and backlog packets record accepted authority boundaries. D1/as-built material remains evidence, not authority.

The user explicitly authorized only TASK-490.25 and TASK-490.26 production repairs, each in a separate commit: `ca46cdd5` repairs active cf-config URL serialization while preserving `/run/postgresql`; `136348e8` propagates the selected local/OIDC mode through the production server unit. Other production, module, migration, SQLx, UI, and design changes remain out of scope.

The combined focused Access scenario and current `integration-access` owner passed at tested revision `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`. Exact commands and runtime evidence are in the coverage map and README. TASK-491 remains integrated. Current outstanding foundation scope is the remaining packet/readiness acceptance criterion; future role proof remains in TASK-490.7. No full Access contract, other owner, or overhaul pass is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Contract is adopted at docs/knowledge/testing/integration-test-contract.md with implementation_status partial, adopted-versus-implemented distinction, and affected AGENTS.md/agent verification links; D1 is not authority
- [x] #2 Coverage map records accepted obligation ID, owner, scenario, independent expected result, evidence and status, plus retirement disposition for every legacy check and CI job
- [x] #3 lib/integration-test exposes lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs } with a documented controller role, production package selection, facts/receipts, bounded waits, fault injection, pinned rendering and failure artifacts; future services/builder/target roles and proving remain in TASK-490.7
- [x] #4 checks/integration-access/default.nix uses { lib, inputs, pkgs, ... }, explicitly registers lifecycle-local-bootstrap and aggregates all registered gates with pkgs.linkFarm; scripts/fixtures/assets stay owner-local
- [x] #5 Focused checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap executes in a real VM with baseline updating disabled and failure propagation; exact command, revision, outcome and evidence are recorded, including linked defects where applicable
- [x] #6 Native discovery exposes meaningful Access WIP without requiring completion; no root tests/, legacyPackages.integration, mkOwner, custom discovery or placeholder owners are added; legacy checks remain until cutover
- [ ] #7 Future bounded child packets record actual dependencies, disjoint fragment paths, shared scenario placement, harness interface, coordinator registration and verified commands before To Do; packets without those conditions remain Backlog
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

Combined Access milestone passed at `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56`. Focused command `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L` passed; aggregate command `nix build .#checks.x86_64-linux.integration-access -L` passed and reused the focused gate (no extra VM). Evidence `/nix/store/xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`: all five cases pass, 285 migrations, module socket `/run/postgresql`, unit `AUTH_MODE=local`, setup-status local/empty, first Admin browser register/sign-in/reload, one persisted session, wrong password rejected without new session/cookie. Screenshot `browser/bootstrap-sign-in/signed-in-after-reload.png`.

The same journal shows independent `crystal-forge-postgres-jobs.service` failure before module role setup (`FATAL: role \"crystal_forge\" does not exist`); task TASK-492 records it. This does not fail the Access gate and is outside the .25/.26 authorization. Access bootstrap alone does not complete the Access contract or other owners.

Exact final foundation branch head `ade5080cd84c7be057b8f0a9d9c490ed8b1fd3d5` was also exercised by the focused Access command and passed all five cases; the owner aggregate passed. This was an extra repeat after the documentation-only/comment follow-up; no further VM run is needed. Its evidence is `/nix/store/0aavcw4vk32c83gljg8y4mz7gi5pqrn4-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-10-17 20:38
---
Coordinator handoff note: TASK-491 package test repair is integrated in commit af39284bdf16d56402f01a64c916f17948e43b02; Access VM scenario runs and fails correctly on TASK-490.25/.26 configuration defects. Evidence and source commit are in implementation notes and coverage map. No packet is ready; this is not an owner pass.
---
<!-- COMMENTS:END -->
