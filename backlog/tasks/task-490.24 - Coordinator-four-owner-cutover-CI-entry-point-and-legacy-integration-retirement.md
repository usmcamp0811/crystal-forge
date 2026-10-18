---
id: TASK-490.24
title: >-
  Coordinator: four-owner cutover, CI entry point, and legacy integration
  retirement
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:39'
updated_date: '2026-10-18 17:43'
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

Goal: four product integration checks (`integration-access`, `integration-delivery`, `integration-assurance`, `integration-upgrade`) that give the accepted end-to-end coverage with the fewest justified scenarios and no duplicate test execution without a specific reason. Rust unit tests stay in Rust. Necessary database-backed Rust tests keep a working database test environment. Additional Nix verification must justify its separate purpose. Developer tools do not automatically stay flake checks.

Two milestones, tracked separately:

1. EARLY CI TRANSITION (selected and authorized by the owner on 2026-10-18; implemented). Implemented integration owners run automatically in GitLab. Retiring legacy integration checks remain available as optional manual jobs while replacements are constructed. Removing a legacy check from automatic CI does not mark its obligations covered. This is an explicit, temporary reduction in automatic integration coverage. It supersedes the earlier requirement to keep all legacy CI scheduling unchanged until all four owners pass, for this milestone only. Scope: `.gitlab-ci.yml` selection, the artifact collection script `packages/ci/integration-owner-build.sh`, and directly related documentation and task records. It changes no production code, scenario assertion, harness semantics, root flake, cache policy, or release policy.
   - Automatic and required: `integration-access` (normal owner aggregate through `packages/ci/public-cache-build.sh`, never a `.run` output).
   - Optional manual (`when: manual`, `allow_failure: true`): `legacy-flake-check` for `integration`, `oidc-auth`, `niks3-cache`, `niks3-write-probe`, `niks3-ui`, `attic-metrics`, `attic-publication`; and `web-ui-check`. `web-ui-baseline-candidates` was already manual.
   - Retained and automatic during migration (a TEMPORARY retention, not a final decision): matrix entries `run-ui-dev-db-check`, `server-regressions`, `nixos-discovery-reliability`, `builder-evaluator-packaging`, `web-ui-test-runner`, `okf-knowledge`; jobs `cve-processing-test`, `dashboard-visibility-tests`, `state-machine-tests`, `coverage-check`, `complexity-check`, and the release/version jobs.
   - Removed: the automatic `web-ui-screenshots-mr-comment` reporter. No replacement bot.
   - Legacy definitions and fixtures stay in the repository.
   The collector `packages/ci/integration-owner-build.sh` had two defects after a failed build: the unterminated last listing record was skipped, and a copy failure replaced the build's exit status. Both are corrected and verified (command-shim exercise and real runs); see the notes.

2. FINAL FOUR-OWNER COMPLETION, MINIMIZATION, AND LEGACY SOURCE RETIREMENT (outstanding). Needs `integration-delivery`, `integration-assurance`, and `integration-upgrade` with real scenarios, and the four-name allowlist in the existing CI entry point. These prerequisites are NOT prerequisites for the early transition. D1 and Upgrade D2 remain applicable.

FINAL MINIMIZATION RULE. Every remaining independent flake check or CI test job must have a distinct necessary purpose. Preserve useful assertions, assign their primary execution ownership, consolidate duplicate execution, and keep developer tools available through appropriate package or app outputs. Classification as "non-integration" alone does not justify a separate check.

Temporary retention during migration is different from a justified final retention decision. A check kept now stays "provisional" in the retirement map until its separate execution is justified. The final cutover must establish: exactly four product integration owners; complete accepted coverage under them; removal of replaced legacy integration definitions and obsolete manual jobs; no task-specific VM proliferation hidden beneath the four aggregate names; a reason for any repeated Rust/database test execution; a reason for every remaining separate non-integration check; and the actual final flake inventory with measured execution and resource costs. Do not delete existing checks arbitrarily before then. Do not merge checks only to reduce names while keeping duplicate work underneath. Multiple attributes can share one Nix derivation, so attribute count alone does not measure duplicate execution. Use the existing retirement map; no new registry or audit task family.

Known examples recorded for the final cleanup (reported, not audited and not pursued in the current pass): `server-regressions` and `state-machine-tests` select some of the same ignored commit tests; `ui-screenshots` is already a package and is described as an image-generation aid without semantic assertions.

Existing prerequisite work: the TASK-490.25/.26 configuration repairs are integrated at `52128b3a63175b712f7a83860355fdeec658a230` and `76e514644505f232e4e47f085db801913d8449ab`. TASK-492 is separate and outside their authorization.

Current coverage, stated accurately: the Access scenario `lifecycle-local-bootstrap` has 16 cases (five bootstrap, five session, six Operator/Viewer admin-denial authorization cases) and passed through the CI job script at `8a857bc872cf331a19036b97a93f2b7741e9728b`. It does not complete cross-environment scope, closed registration, OIDC, broad UI/design, Delivery, Assurance, or Upgrade coverage. TASK-490.2 remains In Progress. The other owners and the final cutover remain incomplete. Child states: TASK-490.1, .25, .26 are Review; TASK-490.2 and this task are In Progress; the others are Backlog. No merge or deployment is authorized.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 [Early CI] The existing CI selection runs `integration-access` automatically and required on merge requests and `main` by building the normal owner aggregate `.#checks.x86_64-linux.integration-access` through packages/ci/public-cache-build.sh with `-L --show-trace --builders '' --max-jobs 1 --cores 2`; a focused scenario or `.run` output is never the gate; no placeholder owner or four-name assertion is added
- [x] #2 [Early CI] `integration`, `oidc-auth`, `niks3-cache`, `niks3-write-probe`, `niks3-ui`, `attic-metrics`, `attic-publication` and `web-ui-check` are optional manual jobs with `when: manual` and `allow_failure: true`; their check attributes, commands, assertions and fixtures are preserved
- [x] #3 [Early CI] The six retained matrix entries, the three database-backed Rust jobs, coverage-check, complexity-check and the release/version jobs remain automatic during migration (temporary retention, not a final decision); the automatic web-ui-screenshots-mr-comment reporter is removed; web-ui-baseline-candidates stays manual
- [x] #4 [Early CI] Owner evidence is exported as real files with `artifacts: when: always`. After a failed owner build the script keeps the original exit status, processes every listed scenario including an unterminated last record, copies only existing `.run` outputs, reports a copy failure without replacing the status, keeps the log, and never launches a `.run` build. A successful build with missing or uncopyable evidence fails the job
- [x] #5 [Early CI] The contract, coverage map (retirement map distinguishing automatic selection from source retirement, with the automatic coverage gaps), flake-check catalog, web-ui-check runbook, verification guide, Access README, TASK-490 and this task state the early-transition policy accurately; no legacy obligation is marked covered
- [x] #6 [Early CI] GitLab lint accepts the file, the effective selection matches the intent for merge-request and main pipelines, script syntax is checked, and the actual Access CI execution path ran once with its evidence inspected and VM-executed versus reused recorded
- [ ] #7 [Final cutover] At final cutover exactly four native checks/integration-{access,delivery,assurance,upgrade}/default.nix owners use { lib, inputs, pkgs, ... }; each normal pkgs.linkFarm aggregate executes ALL registered gates and its complete required set, propagating every failure
- [ ] #8 [Final cutover] CI runs the four owners through checks.x86_64-linux and checks the four-name allowlist in the existing entry point, without a new validation service
- [ ] #9 [Final cutover] Every retired legacy integration check, fixture, and obsolete manual job has a passing replacement recorded in the coverage map or an explicit owner-approved scope change, and is then removed; failing product assertions remain executable and linked
- [ ] #10 [Final cutover] MINIMIZATION: every remaining independent flake check or CI test job has a distinct necessary purpose recorded in the retirement map. Useful assertions have a primary execution owner, duplicate execution is consolidated or justified, developer tools stay available as package or app outputs, and 'non-integration' alone never justifies a separate check. Temporary migration retention is distinguished from justified final retention
- [ ] #11 [Final cutover] The final cutover records complete accepted coverage under the four owners; no task-specific VM proliferation hidden beneath the four aggregate names (each separate VM scenario has a recorded isolation reason); a reason for any repeated Rust/database test execution; and the actual final flake inventory with measured runtime and peak resource cost per owner, exact commands, revisions and real VM outcomes
- [ ] #12 [Final cutover] Coordinator explicitly registers child scenarios and maintains shared lib/CI/docs; scripts/fixtures/assets are owner-local, no root tests/, legacyPackages.integration, mkOwner, custom discovery, complete=true export switch or placeholder owners; meaningful WIP discovery is permitted before cutover
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Current (2026-10-18)

Early CI transition implemented (draft MR !335); criteria #1 to #6 are checked. Final cutover criteria #7 to #12 are unmet and untouched. The task stays In Progress. No merge, deployment, or Done transition is authorized.

Collector correction (this pass). Two defects in `packages/ci/integration-owner-build.sh` after a failed build were reproduced against the reviewed version and fixed. (A) The scenario listing has no final newline, so `while read` dropped the last record; a single-scenario owner collected nothing. Fixed with `|| [ -n "$scenario" ]`. (B) A failed `cp` under `set -e` ended the script with the copy status instead of the build status. Fixed by making `copy_evidence` return an explicit status, turning `set -e` off after a failed build, and reporting and counting each copy failure. After a successful build any missing or uncopyable evidence still exits 1. A temporary command-shim exercise ran the actual script with the real raw listing format (no committed test): the reviewed version passed 7 of 15 cases (it failed one, three and mixed existing scenarios, a real read-only store output, and copy-failure statuses 17/23/1); the corrected version passed 15 of 15. It covers: one, three and mixed existing scenarios without a final newline; missing outputs; a failed evaluation; copy failures that keep status 17 and 23; and successful builds with a missing link, a missing evidence directory, an uncopyable directory and no result link. Only the owner aggregate was ever built. Two real runs of the corrected script on clean revisions also passed (below). A first reading of one case as "success hides a copy failure" was a flaw in the exercise (its shim never triggered); the reviewed script already exited 1 there.

Real runs of the CI job script, VM executed newly each time: at `2c494ac4bf8cc7155899723ee69fcf879ac49248` (10 of 10 cases) and at `8a857bc872cf331a19036b97a93f2b7741e9728b` (16 of 16 cases; adds six TASK-490.2 authorization cases). The latter: exit 0, 25 derivations built including the scenario run, about 110 s wall, 1.2 MB of real evidence files, run output `/nix/store/xld57xqbfpvqpcmbqjij5djsm9l2j0mv-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run`. No real runtime token in the log or evidence.

Final minimization rule recorded in this task, the contract, the coverage map and the parent. The retirement map now marks every `keep` as `keep (provisional)`, adds the recorded (unaudited) overlap between `server-regressions` and `state-machine-tests`, and notes that `ui-screenshots` is already a package. Nothing was deleted or merged.

Earlier validation at `2c494ac4`: GitLab's linter accepted the file (23 jobs; the baseline also has 23); the MR pipeline created the intended jobs (`integration-access` automatic; 7 `legacy-flake-check` entries and `web-ui-check` manual with `allow_failure: true`; no screenshot reporter). The linter's dry-run mode returns an empty pipeline for the unchanged baseline too, so it cannot simulate `only:`. Hosted result: the GitLab `integration-access` job succeeded in MR pipeline 2960470533 at `d28dfc9a` (10 cases, 75 s, artifacts.zip uploaded) and in pipeline 2960613754 at `08a79037f872ec7743293a9d973a21f9aa4acebb` (all 16 cases passed; 58 derivations built, so the VM ran; 74 s; 1.0 MB artifact). The same pipeline for `d28dfc9a` failed `flake-check: [server-regressions]` in `poam_workflows` with no Rust change (it passed at `9c6ce8ff`); recorded as TASK-494. The docs-only commit after `08a79037` is not separately verified.

Unrelated findings: TASK-494 (a `server-regressions` POA&M test failed once with no Rust change). `packages/ci/test_public_cache_build.py` already fails on the unmodified baseline at its `web-ui-check` argv assertion (stale `--keep-failed` expectation). Recorded as TASK-493 (Backlog); not fixed here.

Remaining automatic coverage gaps (deliberate and temporary): cross-environment scope, closed registration, OIDC, forms, preferences, notifications, shell, broad browser/design coverage, authorization beyond the selected admin-only operations, and all Delivery, Assurance, and Upgrade obligations have no automatic integration coverage. Their legacy jobs are manual. Nothing is marked covered.

## Historical (superseded readiness statements)

Before the early transition, the recorded policy was to keep all legacy CI scheduling unchanged until all four owners passed. The owner superseded that for the early milestone. Older records follow.

2026-10-17 organization correction replaced old registration/layout/commands and the incomplete-unexported rule. At that time only Access existed, the foundation was blocked by two Rust package tests, no VM had executed, and all children were Backlog. D1/as-built is not authority; uncertain cases await owner decision. Cutover dependencies were expanded to include TASK-490.25/.26 and the Assurance packets .27 to .29.

Access execution history: bootstrap passed at `b1095c8f2bb2db16d79a2980bb4cd8dd0ce09d56` (evidence `/nix/store/xxvi4m1s6a7ajm4al64sqqb7qvhhidjz-…`), at `ade5080cd84c7be057b8f0a9d9c490ed8b1fd3d5` (`/nix/store/0aavcw4vk32c83gljg8y4mz7gi5pqrn4-…`), and after rebase on `origin/dev` `5ae7feab8a7b372e8efe0d2dd21b6f53f0625064` at `bba718befbc183863781656df2df272e5f735d5a` (`/nix/store/riky1mm0nn185wb26abw7zzshnbml0fp-…`). The ten-case result is at `81d65be745cc9f9777d368900d01fe0f207cb052` (`/nix/store/i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh-…`); the first CI-script run of ten cases is at `2c494ac4` (`/nix/store/cmvnmvc7sbdhd64vj9rd0yqz0iq9lzac-…`). TASK-492 records the unrelated Postgres Jobs first-boot role race; no Access assertion failed.
<!-- SECTION:NOTES:END -->
