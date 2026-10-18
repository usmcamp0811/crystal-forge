---
id: TASK-490.24
title: >-
  Coordinator: four-owner cutover, CI entry point, and legacy integration
  retirement
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:39'
updated_date: '2026-10-18 18:22'
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
Coordinator-owned four-owner completion and source retirement. Final checks.x86_64-linux must contain exactly these FOUR TOTAL attributes: integration-access, integration-delivery, integration-assurance, integration-upgrade. Native Snowfall checks/<owner>/default.nix registration remains unchanged. Four check attributes does not mean only four verification commands.

The owner's source-retirement instruction explicitly supersedes both legacy-source-retention-until-coverage and the interim manual-job policy. Delete the nine named legacy suite directories now after inspecting their consumers. Their accepted product obligations remain in the coverage map as uncovered where no new scenario passes. Do not reproduce the old suites one-for-one or treat their inventory as requirements. The accepted contract/current CF architecture defines integration coverage.

Selected source-removal milestone: nine legacy suites and optional CI jobs removed; useful host browser/design assets relocated to packages/devScripts/web-ui; two XCCDF inputs moved to checks/integration-assurance/fixtures/xccdf without a placeholder owner; 17 non-integration definitions relocated as native package outputs (verify-* except ui-screenshots and okf-knowledge). Checks output decreases from 27 to 1, integration-access only. Its 16 cases remain intact. No final four-owner completeness claim.

Lower-level paths retain useful assertions: verify-server-regressions preserves the exact Rust integration/ignored selections and isolated postgresqlTestHook with doCheck=true. It remains automatic through package-verification. Existing cve-processing-test, dashboard-visibility-tests and state-machine-tests retain disposable database environments. Schema/documentation/packaging/tool verification runs as explicit packages, not flake checks. No custom registry/discovery/framework.

Remaining complex cleanup here: compare ignored commit selections in packages/verify-server-regressions/default.nix and packages/devScripts/default.nix state-machine-test. Choose one primary runner for exact overlaps; preserve distinct test environment evidence or record a reason. Dependency: complete exact selected-test mapping and validate the surviving isolated DB path before removing duplicates. Generic packages/cf-test-suite remains developer tooling; only accepted obligations justify new owner cases, not historical test inventory. No new audit task family.

Final construction remains six planned scenarios across four owners. Extend compatible same-scenario cases; add a VM boundary only for required machine configuration, lifecycle or isolation. Missing owners, forms, notifications, OIDC, cross-environment scope and closed registration remain open under the existing child tasks and their applicable decisions. TASK-492/493/494 do not authorize product fixes. No merge, deployment, production/schema/design change or Done transition.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Automatic integration-access builds its normal owner aggregate through the existing cache wrapper/resource limits and propagates failures; never use a .run output as the gate
- [ ] #2 Source retirement removes checks/integration, oidc-auth, niks3-cache, niks3-write-probe, niks3-ui, attic-metrics, attic-publication, web-ui and web-ui-reconciliation after external consumers are relocated; obsolete legacy CI jobs and dead asset references are removed
- [ ] #3 Lower-level Rust/database/package/schema/documentation/tool verification has explicit working package or CI execution paths outside flake checks; preserve isolated PostgreSQL and normal package tests
- [x] #4 Owner evidence exports real dereferenced files with artifacts always; failed-build collection processes unterminated last records, preserves the original status after copy errors, never rebuilds .run, and a successful build with missing evidence fails
- [ ] #5 Contract, retirement map, verification guide and task records distinguish executed coverage, required gaps, obsolete suites, and lower-level verification retained outside checks; retirement does not mark missing obligations covered
- [ ] #6 Evaluate the actual flake inventory and changed package/import/CI consumers; run retained tooling/schema/doc checks and normal Access CI execution, recording revision, result and evidence reuse/execution
- [ ] #7 FINAL: nix eval .#checks.x86_64-linux --apply builtins.attrNames --json returns exactly integration-access, integration-delivery, integration-assurance and integration-upgrade, with no other attributes
- [ ] #8 FINAL: all four native Snowfall owner aggregates build every explicitly registered scenario gate and prove the complete accepted coverage; no placeholder owners
- [ ] #9 FINAL: all replaced legacy definitions and obsolete manual jobs are removed; preserve necessary assets at their owner/package locations; historical test inventory does not create mandatory product requirements
- [ ] #10 FINAL: useful assertions have primary execution ownership; repeated Rust/database work is consolidated or justified by distinct execution evidence; developer tools use packages/apps and no non-integration flake check attribute remains
- [ ] #11 FINAL: retain the six-scenario starting topology with each extra machine/lifecycle/isolation boundary justified; report measured runtime/resources and the actual final inventory, not merely CI job names or four wrapper aggregates
- [ ] #12 Coordinator owns shared harness, composition, registration and CI; owner-local case fragments reuse same-scenario receipts only; no root tests/, custom discovery, registry or new test framework
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Source-retirement pass selected by the owner. Resume existing TASK-490.1-integration-foundation worktree at 5fa1f4fa; existing openai coordinator lock remains owned by this session. No application changes.
1. Inspect all external consumers of the nine named legacy suites before deletion. Preserve needed browser developer assets and XCCDF fixtures; remove obsolete suite implementations and manual CI jobs.
2. Relocate standalone non-integration verification from checks/ to native package outputs with explicit CI commands. Preserve PostgreSQL hooks and all selected Rust regressions; retain only integration owners in checks/. Do not add placeholders or custom discovery.
3. Update TASK-490, this task, contract and retirement map: final checks.x86_64-linux attrNames must equal the four approved names exactly. Retirement is authorized now without completed replacement coverage; missing accepted obligations remain gaps.
4. Validate moved imports, retained browser tooling, documentation/schema/Nix package commands, database-regression path and unchanged Access aggregate. Capture actual inventory and any complex deduplication left for this task.
5. Commit and push bounded changes to draft MR !335. Leave final task requirements open; do not merge or deploy.

Delegated browser-asset relocation: move the minimal host web-ui-test and design generator dependency closure from checks/web-ui into packages/devScripts/web-ui; adjust only developer runner and design output references. Preserve imported workflow modules and referenced standalone eval-drawer capture tooling. Remove the static dependency on retired VM default.nix. Run host runner selftest through nix develop plus generator selftests and syntax checks; stage owned files only. Coordinator owns legacy deletion, XML fixture relocation, CI/docs/shared harness and task state.

Scoped package-relocation worker: move the 17 nonintegration definitions to packages/verify-<oldname>, except packages/ui-screenshots and packages/okf-knowledge. Preserve assertions and exact server-regressions PostgreSQL/Cargo selection. Inspect test-key consumers before removal. Update only owned executable references and the three root flake imports for UI screenshots/OKF; report other consumers. Stage new package files; evaluate every drvPath and run feasible lightweight package builds without VM/database services, server-regressions, or screenshot builds. Baseline server-regressions drvPath: /nix/store/cj9yfsfs54v821m2zv659lj8wbl9pxl1-crystal-forge-server-regressions-0.1.0.drv.
<!-- SECTION:PLAN:END -->

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

The owner now explicitly authorizes deleting legacy check definitions before all four owners are complete. Prior retention-until-passing-replacement instructions are superseded for this source-removal pass. Final inventory means exactly four TOTAL flake check attributes, not four owners plus non-integration checks. Baseline eval count: 27; active worktree clean at 5fa1f4fa.

Browser-asset slice implemented and staged in TASK-490.1-integration-foundation (no commit). Relocated 15 files to packages/devScripts/web-ui: coverage-manifest.json; design-fixtures.json; design-parity/{generate-design-targets.js,manifest.json,generate-design-targets-test.js,capture-eval-drawer-shell.cjs,eval-drawer.md}; tests/{web-ui-test.sh,integration-test.js,web-ui-test-runner-test.sh,shared-cache-modal-workflow.js,niks3-cache-workflow.js,retained-cache-credential-workflow.js,eval-dialog-workflow.js,eval-policy-matrix-workflow.js}. Updated the three owned Nix consumers. Removed static VM default.nix assertions; retained scenario assertions and other static contracts. Host unsupported-workflow errors now identify retirement rather than print a dead VM command.
Verification: task-local `nix develop -c ...` failed during evaluation with infinite recursion at modules/nixos/crystal-forge/default.nix:13 (pkgs.crystal-forge.default.evaluatorNix) while concurrent package relocation was active. Used `nix develop /home/mcamp/code/crystal-forge/dev -c env -u DB_HOST -u DB_PORT -u DB_USER -u DB_NAME bash -euo pipefail -c '...'` against task-worktree files: host runner selftest passed; generator selftest passed; bash -n for both shell scripts, node --check for every relocated JS/CJS script, and nix-instantiate --parse for all three edited Nix files passed. Unsetting DB_* avoids inherited shell connection values conflicting with the selftest's default assertions; all database calls are mocked. In the same repository environment, `CF_UI_STATIC_CONTRACTS=1 node packages/devScripts/web-ui/tests/integration-test.js` passed, and generator `--validate-manifest` passed (19 primary views, 8 TASK-440 targets). `git diff --cached --check` passed. No browser/server services or baseline generation ran.
Coordinator follow-up: host integration-test.js currently expects packages/devScripts/web-ui/tests/fixtures/U_Anduril_NixOS_STIG_V1R2_Manual-xccdf.xml; supply that path in the separate XML move or provide its new location for updating the read. Remaining executable consumers in checks/web-ui/default.nix and checks/web-ui-reconciliation/default.nix reference relocated files and need coordinator retirement. The other worker has updated packages/verify-web-ui-test-runner/default.nix to the new paths. devenv.nix has two stale comment paths (239/355); docs/knowledge/testing/index.md and docs/knowledge/meta/migration-manifest/testing-operations.md link the relocated eval-drawer runbook. These coordinator-owned files were not edited.

Package-relocation worker handoff (shared TASK-490.1 worktree): all 17 native package drvPaths evaluate. Mapping: checks/<name> -> packages/verify-<name> for builder-cve-contract, builder-evaluator-packaging, config-inspector, config-observer, evaluator-snapshot-isolation, niks3-ca-bundle, nixos-discovery-reliability, nixos-options-metadata, oscal-export, run-ui-dev-db-check, server-regressions, stig, verified-source-evaluator-parity, web-ui-test-runner, xccdf-schema; exceptions checks/ui-screenshots -> packages/ui-screenshots and checks/okf-knowledge -> packages/okf-knowledge. Existing root aliases/imports updated only for UI/OKF. Browser runner imports use packages/devScripts/web-ui/tests/web-ui-test{,-runner-test}.sh and coverage-manifest.json. Existing relative schema/src paths retain the same two-level repository depth. Three existing top-level assertion groups (builder-cve-contract, builder-evaluator-packaging, nixos-options-metadata) moved unchanged into their runCommand build-command argument to prevent Snowfall package namespace recursion. No assertion expressions or Rust/PG selections changed. test-keys removed after repository consumer inspection: no executable file consumers, legacy suites embed/generate distinct fixtures. Server-regressions old/new drvPath IDENTICAL: /nix/store/cj9yfsfs54v821m2zv659lj8wbl9pxl1-crystal-forge-server-regressions-0.1.0.drv; default.nix old/new Git blob hash both 8dbc8cfa410ad148f39def97a7e40c5f43374cce.

Coordinator-owned consumers to update: .gitignore:133 fixture flake.lock exception -> /packages/verify-verified-source-evaluator-parity/fixture/flake.lock; packages/ci/test_public_cache_build.py:216,220 builder-evaluator-packaging check argv -> packages.x86_64-linux.verify-builder-evaluator-packaging; .gitlab-ci.yml retained six-name check matrix/dispatch -> explicit native package builds. coverage-audit.py:281 retains an immutable-baseline prose transformation string referencing the old command; this is preservation-audit data, not a command invocation, and was deliberately not changed.

Verification exit 0: nix eval --json --apply 'p: builtins.listToAttrs (map (n: { name = n; value = p.${n}.drvPath; }) [ "verify-builder-cve-contract" "verify-builder-evaluator-packaging" "verify-config-inspector" "verify-config-observer" "verify-evaluator-snapshot-isolation" "verify-niks3-ca-bundle" "verify-nixos-discovery-reliability" "verify-nixos-options-metadata" "verify-oscal-export" "verify-run-ui-dev-db-check" "verify-server-regressions" "verify-stig" "verify-verified-source-evaluator-parity" "verify-web-ui-test-runner" "verify-xccdf-schema" "ui-screenshots" "okf-knowledge" ])' .#packages.x86_64-linux. Initial evaluation failed with namespace recursion, repaired by assertion deferral above. Exit 0: nix build --no-link -L --builders '' --max-jobs 1 --cores 2 .#verify-builder-cve-contract .#verify-config-inspector .#verify-config-observer .#verify-evaluator-snapshot-isolation .#verify-nixos-discovery-reliability .#verify-nixos-options-metadata .#verify-run-ui-dev-db-check .#verify-stig .#verify-verified-source-evaluator-parity .#verify-web-ui-test-runner (only runner rebuilt; others already realized). Exit 0: same build flags .#verify-niks3-ca-bundle (runner compiled; 13/13 selected ca_bundle Rust tests passed, all three dataset reports identities_equal=true). Exit 0: same flags .#verify-oscal-export .#verify-builder-evaluator-packaging.moduleValidation (already realized). git diff --check and git diff --cached --check exit 0.

Blocking coordinator documentation repairs: nix build --no-link -L --builders '' --max-jobs 1 --cores 2 .#okf-knowledge .#verify-oscal-export exits 1 in OKF validation after 58/58 self-tests, diagram scan CANDIDATES=77 UNCLASSIFIED=0 STALE=0, and 216 Mermaid parses with 0 errors. 52 failures are stale links / retained-path manifest rows / new moved README manifest entries. Knowledge files: caches/niks3-ca-bundle-compatibility.md, caches/niks3-cache.md, evaluation/bulk-evaluator-resource-planning.md, meta/migration-manifest/testing-operations.md, testing/flake-checks.md, testing/index.md. Full log: nix log /nix/store/z252zsvjcg1cbl82qmvdnap8izxnsjx1-okf-knowledge-validation.drv. Did not modify those coordinator-owned docs. Full verify-builder-evaluator-packaging and verify-xccdf-schema builds deferred: dry-run requires cf-server-core Rust rebuild, outside lightweight pass. No server-regressions build, VM, database service, screenshot build, commit, stash, or reset.

Source retirement now underway at baseline 5fa1f4fa: 27 evaluated check attrs. The nine legacy suite consumers were inspected; browser tooling and XML relocated before deleting suites. Nonintegration definitions moved to native packages. verify-server-regressions is byte-identical to the retired default.nix and has the same drvPath, preserving PostgreSQL hooks and exact selection. The overlap with state-machine-tests is explicitly outstanding within this task, not hidden or declared resolved.
<!-- SECTION:NOTES:END -->
