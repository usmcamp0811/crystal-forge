---
id: TASK-490.24
title: >-
  Coordinator: four-owner cutover, CI entry point, and legacy integration
  retirement
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:39'
updated_date: '2026-10-18 18:52'
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
- [x] #2 Source retirement removes checks/integration, oidc-auth, niks3-cache, niks3-write-probe, niks3-ui, attic-metrics, attic-publication, web-ui and web-ui-reconciliation after external consumers are relocated; obsolete legacy CI jobs and dead asset references are removed
- [ ] #3 Lower-level Rust/database/package/schema/documentation/tool verification has explicit working package or CI execution paths outside flake checks; preserve isolated PostgreSQL and normal package tests
- [x] #4 Owner evidence exports real dereferenced files with artifacts always; failed-build collection processes unterminated last records, preserves the original status after copy errors, never rebuilds .run, and a successful build with missing evidence fails
- [x] #5 Contract, retirement map, verification guide and task records distinguish executed coverage, required gaps, obsolete suites, and lower-level verification retained outside checks; retirement does not mark missing obligations covered
- [ ] #6 Evaluate the actual flake inventory and changed package/import/CI consumers; run retained tooling/schema/doc checks and normal Access CI execution, recording revision, result and evidence reuse/execution
- [ ] #7 FINAL: nix eval .#checks.x86_64-linux --apply builtins.attrNames --json returns exactly integration-access, integration-delivery, integration-assurance and integration-upgrade, with no other attributes
- [ ] #8 FINAL: all four native Snowfall owner aggregates build every explicitly registered scenario gate and prove the complete accepted coverage; no placeholder owners
- [x] #9 FINAL: all replaced legacy definitions and obsolete manual jobs are removed; preserve necessary assets at their owner/package locations; historical test inventory does not create mandatory product requirements
- [ ] #10 FINAL: useful assertions have primary execution ownership; repeated Rust/database work is consolidated or justified by distinct execution evidence; developer tools use packages/apps and no non-integration flake check attribute remains
- [ ] #11 FINAL: retain the six-scenario starting topology with each extra machine/lifecycle/isolation boundary justified; report measured runtime/resources and the actual final inventory, not merely CI job names or four wrapper aggregates
- [ ] #12 Coordinator owns shared harness, composition, registration and CI; owner-local case fragments reuse same-scenario receipts only; no root tests/, custom discovery, registry or new test framework
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Clean-slate consolidation iteration selected by latest owner request. Baseline2c34186e, clean existing worktree, same coordinator lock. Earlier one-for-one verify package relocation is rejected and superseded.
1. Review all15verify packages for unique assertions vs Rust/component/CI owners; measure exact duplicate selections and before drv inventory.
2. Eliminate verify public outputs. Keep unique component assertions in corresponding ordinary build/test mechanisms or explicit existing CI commands; no new registry/aggregator. Consolidate PostgreSQL regressions into existing isolated state-machine workflow, remove exact repeated work and preserve SQLx privileges/migration rehearsals.
3. Inventory actual API, Dioxus, configuration, agent/builder/cache/evidence/upgrade product surfaces against accepted authorities; update existing coverage map with truthful dispositions, exact decisions and no old suite authority.
4. Preserve and execute existing Access scenario. Check dependencies and implement one next runnable existing child coverage slice (Access form success if Delivery prerequisite roles cannot be completed safely in this bounded iteration).
5. Verify all real commands/statuses; record check/package/job counts, derivation reduction and repeated work removed. Commit bounded changes to draft !335; keep parent and final requirements incomplete.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Current source-retirement result (2026-10-18)

Implementation commit 0105e512baa80ace5f181918bbb72951bf506a50 in existing TASK-490.1-integration-foundation worktree/branch and draft MR !335. The owner's explicit source-retirement instruction supersedes all prior legacy-source and manual-coexistence retention rules. Task remains In Progress; no merge/deploy/Done claim. LOCK: openai on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation.

Actual inventory command `nix eval .#checks.x86_64-linux --apply builtins.attrNames --json`: ["integration-access"]. Before: 27. After: 1. The final exact four-name test is NOT satisfied: Delivery, Assurance and Upgrade have no default.nix. Retained Assurance XML assets do not register a placeholder.

Deleted all nine legacy suites: checks/{integration,oidc-auth,niks3-cache,niks3-write-probe,niks3-ui,attic-metrics,attic-publication,web-ui,web-ui-reconciliation}. Deleted orphan lib/cache-test-node and unused test-key placeholders after checking consumers. Old suite VM/test implementations, VM-only validators and baseline artifacts are removed, not copied into new owners. Historical source is in Git at 5fa1f4fa.

Assets retained: 15 host browser/design tooling files under packages/devScripts/web-ui (runner, monolithic developer workflow script and its imported modules, fixture/manifest, generator/tests, capture runbook). External devScripts/design-target package/app consumers updated. Both XCCDF XML revisions moved to checks/integration-assurance/fixtures/xccdf; the developer harness reads V1R2 there. Approved design sources remain unchanged. Retired OIDC realm copy is unnecessary: developer OIDC stack owns its separate realm.

All 17 nonintegration definitions moved to native package outputs. Fifteen are packages/verify-<former-name>; ui-screenshots and okf-knowledge keep their existing package names. The source directory table is in docs/knowledge/testing/flake-checks.md. No standalone nonintegration definition remains under checks. No registry/custom Snowfall discovery/new framework. Three assertion groups are forced in their existing build-command argument rather than before namespace construction, preserving expressions while avoiding recursive package lookup.

CI: removed legacy-flake-check (seven entries), web-ui-check and web-ui-baseline-candidates. Renamed retained six-entry matrix to package-verification with explicit package targets and unchanged runner/auth/local limits/cache-wrapper selection. Existing cve-processing-test, dashboard-visibility-tests, state-machine-tests retain their isolated DB launchers. Coverage/complexity/release unchanged.

Verification passed: all17 drvPaths; ten lightweight contract/tool package builds; CA compatibility (13/13 selected Rust tests and three matching identity reports); OSCAL and builder module validation; .#okf-knowledge; task-local nix develop, browser runner selftest, generator tests/static contracts, syntax; updated CI routing/cache-policy script; GitLab lint (valid, no errors/warnings, correct MR/main only rules); git diff --check. No stale executable moved-check path remains.

Access: `sh packages/ci/integration-owner-build.sh integration-access` exited0 at clean 0105e512. VM EXECUTED, not reused;16/16 cases passed and Viewer browser result passed. Existing assertions and registration unchanged. VM script24.23seconds. Evidence /nix/store/n68lcq6xxab1rhhfymp7yz4pj19hff95-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run plus integration-evidence/integration-access/. Documentation-only follow-up does not require rerunning VM.

Incomplete verification: full verify-server-regressions exceeded600seconds after isolated PG started, migration-prefix rehearsals succeeded, assignment_semantics13/13 and zombie-repair1/1 passed, and composite_policy continued. No full pass claimed. Its default.nix is byte-identical (Git blob8dbc8cfa410ad148f39def97a7e40c5f43374cce) and drvPath unchanged (cj9yfsfs54v821m2zv659lj8wbl9pxl1-crystal-forge-server-regressions-0.1.0.drv). Full builder-evaluator-packaging/XCCDF command also exceeded600seconds compiling core server dependency. Preserve these visible gaps; obtain longer build window or exact-head CI. AC#3/#6 remain unchecked for full verification; do not move task Review.

Remaining complex migrations (THIS task, no new task family): exact ignored-test overlap mapping between packages/verify-server-regressions/default.nix and packages/devScripts/default.nix state-machine-test; assign primary owner and validate surviving isolated DB environment before removing duplicate invocations. Existing Rust/crate test execution should own unit assertions. Package relocation removes check attrs, not duplicated execution. No historical Python test inventory is automatically a product requirement. Accepted owner gaps continue through existing .2-.23/.27-.29 tasks with six planned scenarios; no extra VMs added.

## Historical milestones

Access bootstrap first failed at fc930335; owner-authorized .25/.26 configuration repairs (52128b3a,76e51464) enabled bootstrap. Sessions passed at81d65be7 and CI-path2c494ac4. Authorization extended same scenario to16 cases at8a857bc8; hosted job passed at08a79037. No unrelated product repair occurred; TASK-492/493/494 remain separate. Collector fixes6d7e7a92 handle unterminated listing records and copy-error status preservation (15/15 temporary shim cases passed). Earlier manual-job and provisional check-attribute retention policies are superseded, not current.
<!-- SECTION:NOTES:END -->
