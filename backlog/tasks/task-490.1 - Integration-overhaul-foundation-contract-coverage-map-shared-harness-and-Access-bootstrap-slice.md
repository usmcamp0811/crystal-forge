---
id: TASK-490.1
title: >-
  Integration overhaul foundation: contract, coverage map, shared harness, and
  Access bootstrap slice
status: Review
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 16:13'
updated_date: '2026-10-18 15:42'
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
Problem: children need the adopted integration contract, coverage/retirement map, shared harness, and a real Access workflow. The foundation milestone now proves fresh production server/database startup, migrations, first-admin local bootstrap, embedded-UI sign-in, reload persistence and rejected-login behavior through the normal NixOS module. Risk: medium; this test-only milestone does not require a live preview.

Architecture: native Snowfall checks/integration-access/default.nix explicitly registers scenarios and aggregates every registered gate with pkgs.linkFarm. Shared helpers live under lib/integration-test/ and native checks call lib.crystal-forge.makeIntegrationTestHarness { inherit pkgs inputs; }. Packets contribute disjoint Python/browser fragments; coordinator owns Nix/Python composition, browserSpecs and registration. Each separate scenario gets a fresh VM/database. Later cases in the SAME scenario MAY reuse earlier real records/identities through receipts; independent derivations MUST NOT share live state. No root tests/, legacyPackages.integration, mkOwner, custom discovery or placeholder owners.

Only the controller role is implemented for this milestone. Services/builder/target roles and their Delivery proving workflow remain in TASK-490.7, composed in delivery-lifecycle. Coordinator owns shared interfaces, owner registration, contract/coverage map and affected agent/testing documentation. Non-goals: generic DSL, generated registry, coverage database, new CI service, legacy cutover, unrelated product/module fixes and design edits. Production repair scope remains limited to separately authorized TASK-490.25/.26; their rebased repair commits are 52128b3a63175b712f7a83860355fdeec658a230 and 76e514644505f232e4e47f085db801913d8449ab. No other production, module, migration, SQLx, UI or design changes are authorized. Adopted requirements control expectations; D1/as-built documents are evidence, not acceptance authority.

Final tested source: 81d65be745cc9f9777d368900d01fe0f207cb052. Both `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L` and `nix build .#checks.x86_64-linux.integration-access -L` passed. Evidence: /nix/store/i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/. result.json records 10 passed cases: five bootstrap cases followed by five TASK-490.2 session cases, 292 migrations, database.unixSocketClients=22 and exact admin/session receipts. The session subset has explicit current user authority. This does not complete the full Access contract, the remaining .2 authorization work or other owners. TASK-492 remains unchanged and outside the repair scope; no new product defect was found.

AC #7 is complete for the immediately reachable controller Access packets .2/.3/.4. Their task records were directly inspected for concrete interfaces/facts, disjoint paths, lifecycle-local-bootstrap placement, coordinator registration ownership, focused verification and exact remaining readiness gaps. The current tested pin is 81d65be745cc9f9777d368900d01fe0f207cb052. .2 sessions is implemented and verified; Operator/Viewer cases are unimplemented, while only uncertain cross-environment endpoint/scope and closed-registration policy cases require decisions. .3/.4 remain Backlog and unselected, with their exact form/design/visual-interface gaps. Foundation packet preparation does not require future Delivery/Assurance/Upgrade dependencies. All foundation acceptance criteria are verified; awaiting review in existing draft MR !335, not merged or Done.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Contract is adopted at docs/knowledge/testing/integration-test-contract.md with implementation_status partial, adopted-versus-implemented distinction, and affected AGENTS.md/agent verification links; D1 is not authority
- [x] #2 Coverage map records accepted obligation ID, owner, scenario, independent expected result, evidence and status, plus retirement disposition for every legacy check and CI job
- [x] #3 lib/integration-test exposes lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs } with a documented controller role, production package selection, facts/receipts, bounded waits, fault injection, pinned rendering and failure artifacts; future services/builder/target roles and proving remain in TASK-490.7
- [x] #4 checks/integration-access/default.nix uses { lib, inputs, pkgs, ... }, explicitly registers lifecycle-local-bootstrap and aggregates all registered gates with pkgs.linkFarm; scripts/fixtures/assets stay owner-local
- [x] #5 Focused checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap executes in a real VM with baseline updating disabled and failure propagation; exact command, revision, outcome and evidence are recorded, including linked defects where applicable
- [x] #6 Native discovery exposes meaningful Access WIP without requiring completion; no root tests/, legacyPackages.integration, mkOwner, custom discovery or placeholder owners are added; legacy check definitions and fixtures remain until final retirement (their automatic CI selection was changed separately by the owner-authorized TASK-490.24 early transition)
- [x] #7 Immediately reachable controller Access packets .2/.3/.4 are inspected and record tested pin 81d65be745cc9f9777d368900d01fe0f207cb052, concrete harness/facts/receipts, disjoint fragments, lifecycle-local-bootstrap placement, coordinator composition/registration ownership, focused command and exact readiness gaps; .2 sessions is verified under explicit user authority; preparation does not require future Delivery/Assurance/Upgrade dependencies
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Awaiting review: existing draft MR !335, https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335. The foundation lock is released for review. All seven own acceptance criteria are verified, including direct inspection of the .2/.3/.4 controller packets. This is not the whole integration overhaul or a merge/Done claim.

Final verification at 81d65be745cc9f9777d368900d01fe0f207cb052:
- `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`: PASS.
- `nix build .#checks.x86_64-linux.integration-access -L`: PASS.
- Evidence /nix/store/i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/result.json: status passed, 10/10 cases (five bootstrap, then five sessions), 292 migrations, database.unixSocketClients=22; admin and exact revoked/retained/browser-sign-out session receipts are present.
- `SQLX_OFFLINE=true nix develop -c cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib database_url_tests`: PASS, 2/2 including SQLx password in the rebuilt URL.
- Normal production package checks stayed enabled in the VM build: 1830 passed, 0 failed, 767 ignored.
- Existing okf validation passed: 58 tests, 192 concepts, 216 Mermaid blocks.
These execution results were supplied by the coordinator; task-record finalization did not rerun the VM/package checks. result.json was read directly during finalization.

Uploaded review artifacts:
- Scenario result: https://gitlab.com/-/project/70402481/uploads/2ab7a8febcfb1aa1386364464decc2f2/result.json
- Browser result: https://gitlab.com/-/project/70402481/uploads/5d551029e97b8b3dd534f312deb35b5f/result.json
- Screenshot: https://gitlab.com/-/project/70402481/uploads/ec258b80b8a107c27d32b9f5c4cf3109/signed-out-protected-navigation.png

### Historical
Initial organization/preflight was recorded from origin/dev 33de73b013b735304dfac9c69745a074a3e76ebd. Notes reported preserved pre-existing dev files, no main worktree and /dev/kvm present; those were historical observations. Two unchanged Rust package tests initially blocked execution. The shared entry requires inputs from the check because Snowfall library-loader inputs omit self.

Native Snowfall evaluation found the prior 26 checks plus integration-access; owner/focused drvPath and dry-run resolved production server derivation 9pihfg4v8mp6rb4qnp148acv5cqsrb43-cf-server-0.3.0.drv without VM execution. Syntax/ast.parse, git diff --check and then-current okf validation passed (58 tests, 191 concepts, 216 Mermaid blocks). flake.nix matched base. Commit 683c86e03ef14b37e69c00030d6d17660398324d preserved unexecuted WIP in draft MR !335.

TASK-491 af39284bdf16d56402f01a64c916f17948e43b02 (MR !336) repaired package tests; the package then reported 1794 passed, 0 failed, 682 ignored. Access runs subsequently failed startup before migrations/HTTP: source a6c8922650bf7f0ff643d181422c0092cfc43242-dirty produced runtime yfc7pkz5zm8k29b82zzfhm2i8f1gn348; clean 39a6a54b651a8b77fc3416b73195e363a782fa2e produced vcfbia521bjad2a85yca3h3n9i4m4ikf; fc930335878279ac426510359c039dfd8961dc13 produced 1d5178gxpdv7j4d20dddddr7ik0pq0p3. Each identifies a vm-test-run-cf-it-access-lifecycle-local-bootstrap-run output. Generated config/journal recorded /run/postgresql, missing server.auth_mode, AUTH_MODE=unset and `both host and hostaddr are missing`. No OIDC HTTP response/browser result was observed. Correct assertions stayed and .25/.26 were linked. Pending-readiness statements and the dated first comment describe those revisions only.

Reviews split .20 triage, .21 plan, .27 waiver, .28 acceptance and .29 verification/closure. Separate-run proposals prohibited cross-run runtime sharing; same-scenario fragments now use earlier real records via receipts. The controller milestone never required .7 roles to be complete.

After authorized .25/.26 repairs, b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56 passed the focused/aggregate gates; runtime xxvi4m1s6a7ajm4al64sqqb7qvhhidjz recorded five bootstrap cases, 285 migrations, socket configuration, AUTH_MODE=local and first-admin browser/session behavior. Screenshot: browser/bootstrap-sign-in/signed-in-after-reload.png. TASK-492 recorded an independent Postgres Jobs role-ordering defect. ade5080cd84c7be057b8f0a9d9c490ed8b1fd3d5 repeated that pass with runtime 0aavcw4vk32c83gljg8y4mz7gi5pqrn4. Rebase onto origin/dev 5ae7feab8a7b372e8efe0d2dd21b6f53f0625064 produced tested bba718befbc183863781656df2df272e5f735d5a, runtime riky1mm0nn185wb26abw7zzshnbml0fp and 292 migrations; repairs became 52128b3a63175b712f7a83860355fdeec658a230 and 76e514644505f232e4e47f085db801913d8449ab. At that point AC #7 awaited packet inspection and sessions had not run.

The first session UI attempt at 53718683 failed because the Coach overlay interfered with interaction and a promise was unhandled. The test now uses the normal UI Close coach action and Promise.all, with CSRF header-file redaction. No production change or new product defect was needed. The final 81d65be run passed; TASK-492 is unchanged.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-10-17 20:38
---
Coordinator handoff note: TASK-491 package test repair is integrated in commit af39284bdf16d56402f01a64c916f17948e43b02; Access VM scenario runs and fails correctly on TASK-490.25/.26 configuration defects. Evidence and source commit are in implementation notes and coverage map. No packet is ready; this is not an owner pass.
---

created: 2026-10-18 14:39
---
Current review record (2026-10-18): the earlier 2026-10-17 failure comment is historical. Focused Access and owner aggregate PASS at 81d65be745cc9f9777d368900d01fe0f207cb052 with 10 cases, 292 migrations and database.unixSocketClients=22. All seven foundation criteria, including inspected .2/.3/.4 controller packets, are verified. TASK-490.1 is Review in existing draft MR !335, not Done. .2 remains In Progress for unimplemented authorization; .3/.4 remain unselected Backlog. Final evidence and uploaded artifacts are recorded in notes.
---

author: openai
created: 2026-10-18 15:42
---
Early CI transition note (2026-10-18): AC #6 means legacy check definitions and fixtures remain until final retirement. Their automatic CI selection was changed separately by the owner-authorized TASK-490.24 early transition (integration-access automatic and required; the legacy integration jobs optional manual). The 2026-10-17 comment above that says no packet is ready is historical. This does not change the foundation's seven verified criteria.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Foundation ready for review
Adopted the partial integration contract and coverage/retirement map, implemented the controller harness and explicit native Access registration, and verified production local bootstrap. Inspected the immediately reachable .2/.3/.4 packets and completed the bounded packet-preparation requirement. TASK-490.2's selected session subset is composed and verified; remaining authorization work stays in that task.

## Verification
At 81d65be745cc9f9777d368900d01fe0f207cb052, focused lifecycle-local-bootstrap and Access aggregate passed: 10 cases, 292 migrations, database.unixSocketClients=22 and exact identity receipts. Targeted database_url_tests passed 2/2 including SQLx password reconstruction. Enabled production checks reported 1830 passed, 0 failed, 767 ignored. Existing okf validation passed 58 tests, 192 concepts and 216 Mermaid blocks. Runtime: i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh. Uploaded scenario/browser results and signed-out navigation screenshot are linked in notes.

## Review scope
Existing draft MR !335: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335. All seven foundation criteria are verified. Future roles/owners and remaining authorization/form/shell obligations are outside this milestone. TASK-492 remains unchanged. Awaiting review; not merged or Done.
<!-- SECTION:FINAL_SUMMARY:END -->
