---
id: TASK-490.24
title: >-
  Coordinator: four-owner cutover, CI entry point, and legacy integration
  retirement
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:39'
updated_date: '2026-10-18 15:41'
labels:
  - testing
  - integration
  - coordinator
dependencies:
  - TASK-490.1
  - TASK-490.2
  - TASK-490.3
  - TASK-490.4
  - TASK-490.5
  - TASK-490.6
  - TASK-490.7
  - TASK-490.8
  - TASK-490.9
  - TASK-490.10
  - TASK-490.11
  - TASK-490.12
  - TASK-490.13
  - TASK-490.14
  - TASK-490.15
  - TASK-490.16
  - TASK-490.17
  - TASK-490.18
  - TASK-490.19
  - TASK-490.20
  - TASK-490.21
  - TASK-490.22
  - TASK-490.23
  - TASK-490.25
  - TASK-490.26
  - TASK-490.27
  - TASK-490.28
  - TASK-490.29
modified_files:
  - .gitlab-ci.yml
  - packages/ci/integration-owner-build.sh
  - packages/ci/test_public_cache_build.py
  - .gitignore
  - AGENTS.md
  - checks/integration-access/default.nix
  - checks/integration-access/README.md
  - docs/knowledge/testing/integration-coverage-map.md
  - docs/knowledge/testing/integration-test-contract.md
  - docs/knowledge/testing/flake-checks.md
  - docs/knowledge/testing/web-ui-check.md
  - docs/knowledge/testing/test-plan.md
  - docs/agents/verification.md
parent_task_id: TASK-490
priority: high
type: task
ordinal: 536000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Coordinator-owned CI entry point and cutover task. Owns the four native Snowfall owner registrations, the CI selection, legacy-gate retirement, the coverage/retirement map, and acceptance verification. Do not delegate owner registration/shared-library/CI edits or add placeholder owners.

Two milestones, tracked separately:

1. EARLY CI TRANSITION (selected and authorized by the owner on 2026-10-18; implemented in this task now). Implemented integration owners run automatically in GitLab. Retiring legacy integration checks remain available as optional manual jobs while replacements are constructed. Removing a legacy check from automatic CI does not mark its obligations covered. This is an explicit, temporary reduction in automatic integration coverage. It supersedes the earlier requirement to keep all legacy CI scheduling unchanged until all four owners pass, for this milestone only. Scope: `.gitlab-ci.yml` selection, the artifact collection script `packages/ci/integration-owner-build.sh`, and directly related documentation and task records. It changes no production code, scenario assertion, harness semantics, root flake, cache policy, or release policy.
   - Automatic and required: `integration-access` (normal owner aggregate through `packages/ci/public-cache-build.sh`, never a `.run` output).
   - Optional manual (`when: manual`, `allow_failure: true`): `legacy-flake-check` for `integration`, `oidc-auth`, `niks3-cache`, `niks3-write-probe`, `niks3-ui`, `attic-metrics`, `attic-publication`; and `web-ui-check`. `web-ui-baseline-candidates` was already manual.
   - Unchanged and automatic: matrix entries `run-ui-dev-db-check`, `server-regressions`, `nixos-discovery-reliability`, `builder-evaluator-packaging`, `web-ui-test-runner`, `okf-knowledge`; jobs `cve-processing-test`, `dashboard-visibility-tests`, `state-machine-tests`, `coverage-check`, `complexity-check`, and the release/version jobs.
   - Removed: the automatic `web-ui-screenshots-mr-comment` reporter. No replacement bot.
   - Legacy definitions and fixtures stay in the repository.

2. FINAL FOUR-OWNER COMPLETION AND LEGACY SOURCE RETIREMENT (outstanding; not selected for this pass). Needs `integration-delivery`, `integration-assurance`, and `integration-upgrade` with real scenarios; passing replacement coverage for every retired legacy obligation (or an explicit decision that removes the requirement); the four-name allowlist in the existing CI entry point; per-owner runtime and resource records; and deletion of legacy definitions and fixtures. These prerequisites are NOT prerequisites for the early transition. D1 and Upgrade D2 remain applicable.

Existing prerequisite work: the TASK-490.25/.26 configuration repairs are integrated at `52128b3a63175b712f7a83860355fdeec658a230` and `76e514644505f232e4e47f085db801913d8449ab`. TASK-492 is separate and outside their authorization.

Current coverage, stated accurately: the Access scenario `lifecycle-local-bootstrap` has ten cases (five bootstrap, five session). It does not complete authorization, registration-policy, OIDC, broad UI/design, Delivery, Assurance, or Upgrade coverage. TASK-490.2 remains incomplete. The other owners and the final cutover remain incomplete. Child states: TASK-490.1, .25, .26 are Review; TASK-490.2 is In Progress; the others are Backlog. No merge or deployment is authorized.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 [Early CI] The existing CI selection runs `integration-access` automatically and required on merge requests and `main` by building the normal owner aggregate `.#checks.x86_64-linux.integration-access` through packages/ci/public-cache-build.sh with `-L --show-trace --builders '' --max-jobs 1 --cores 2`; a focused scenario or `.run` output is never the gate; no placeholder owner or four-name assertion is added
- [ ] #2 [Early CI] `integration`, `oidc-auth`, `niks3-cache`, `niks3-write-probe`, `niks3-ui`, `attic-metrics`, `attic-publication` and `web-ui-check` are optional manual jobs with `when: manual` and `allow_failure: true`; their check attributes, commands, assertions and fixtures are preserved
- [ ] #3 [Early CI] The six retained matrix entries, the three database-backed Rust jobs, coverage-check, complexity-check and the release/version jobs remain automatic; the automatic web-ui-screenshots-mr-comment reporter and its dependency wiring are removed; web-ui-baseline-candidates stays manual
- [ ] #4 [Early CI] Owner evidence is exported as real files (Nix store symlinks dereferenced) with `artifacts: when: always`; after a failed owner build the script keeps the original exit status, resolves registered scenarios' `.run.outPath` through evaluation, copies only outputs that already exist, keeps the build log, and never launches a `.run` build; a successful build whose evidence is missing fails the job
- [ ] #5 [Early CI] The contract, coverage map (retirement map distinguishing automatic selection from source retirement, with the automatic coverage gaps), flake-check catalog, web-ui-check runbook, verification guide, Access README, TASK-490 and this task state the early-transition policy accurately; no legacy obligation is marked covered
- [ ] #6 [Early CI] GitLab lint accepts the file, the effective selection matches the intent for merge-request and main pipelines, script syntax is checked, and the actual Access CI execution path ran once with its evidence inspected and VM-executed versus reused recorded
- [ ] #7 [Final cutover] At final cutover exactly four native checks/integration-{access,delivery,assurance,upgrade}/default.nix owners use { lib, inputs, pkgs, ... }; each normal pkgs.linkFarm aggregate executes ALL registered gates and its complete required set, propagating every failure
- [ ] #8 [Final cutover] CI runs the four owners through checks.x86_64-linux and checks the four-name allowlist in the existing entry point, without a new validation service
- [ ] #9 [Final cutover] Every retired legacy check/job has a passing replacement recorded in the coverage map or explicit owner-approved scope change; failing product assertions remain executable and linked; legacy definitions and fixtures are deleted only then
- [ ] #10 [Final cutover] Non-integration checks retain their own attributes and explicit classification
- [ ] #11 [Final cutover] Runtime, peak resources, exact commands, revisions, real VM outcomes and .run evidence are recorded per owner
- [ ] #12 [Final cutover] Coordinator explicitly registers child scenarios and maintains shared lib/CI/docs; scripts/fixtures/assets are owner-local, no root tests/, legacyPackages.integration, mkOwner, custom discovery, complete=true export switch or placeholder owners; meaningful WIP discovery is permitted before cutover
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old registration/layout/commands and incomplete-unexported rule. Coordinator writer, not Luna. Only Access exists now; remaining native owner default.nix files require meaningful scenarios, no placeholders. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. All children Backlog, none ready. Future packets need actual dependency commit(s) and verified commands before To Do. TASK-490.20/.21 finer splitting remains a dispatch blocker without scope expansion now. D1/as-built is not authority; uncertain cases await owner decision. No delegation or executed slice is claimed.

Cutover dependencies expanded 2026-10-17 to include the two product blockers discovered by the correct Access foundation slice (TASK-490.25/.26) and the decomposed Assurance work packets (.27-.29). Passing or explicit owner scope approval is still required before retiring the legacy gates. No dependent task is ready.

At tested source revision b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56, focused Access VM and `nix build .#checks.x86_64-linux.integration-access -L` both passed. Current runtime evidence is `/nix/store/xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. Only the bootstrap slice is covered. TASK-492 records the unrelated one-shot Postgres Jobs first-boot role race; no Access assertion failed.

The Access focused scenario and owner aggregate were repeated at final documentation/comment revision `ade5080cd84c7be057b8f0a9d9c490ed8b1fd3d5`; both pass. The evidence output is `/nix/store/0aavcw4vk32c83gljg8y4mz7gi5pqrn4-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. This repeat verifies the final exact head; it is not evidence for any missing Access contract or other owner.

After rebase, the current origin/dev base is `5ae7feab8a7b372e8efe0d2dd21b6f53f0625064`. Both focused Access and owner aggregate passed at rebased source `bba718befbc183863781656df2df272e5f735d5a`; evidence `/nix/store/riky1mm0nn185wb26abw7zzshnbml0fp-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run/`. Cutover remains blocked by incomplete Delivery/Assurance/Upgrade owners and D1/D2, not by the Access bootstrap.
<!-- SECTION:NOTES:END -->
