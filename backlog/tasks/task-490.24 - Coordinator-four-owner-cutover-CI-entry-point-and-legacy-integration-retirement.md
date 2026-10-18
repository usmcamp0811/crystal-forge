---
id: TASK-490.24
title: >-
  Coordinator: four-owner cutover, CI entry point, and legacy integration
  retirement
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:39'
updated_date: '2026-10-18 20:44'
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
Coordinator-owned final four-owner completion and minimal lower-level testing. Final checks.x86_64-linux must contain exactly FOUR TOTAL native Snowfall attributes: integration-access, integration-delivery, integration-assurance, integration-upgrade. There is currently only Access. Four attributes is not four test commands.

Latest owner explicitly rejects the preservation-first relocation: all fifteen packages/verify-* wrappers are eliminated rather than renamed or collected behind a generic output. Useful assertions have component-level execution ownership. Normal Rust tests remain; important isolated PostgreSQL, concurrency, transaction, security and protocol tests remain. Historical suite names are not product requirements.

Current lower-level architecture: core server ordinary tests/postCheck own cf-config unit/CA compatibility and real Nix adapter/protocol assertions (core only, not embedded); nixos-options-metadata and oscal-fixture ordinary builds validate their own artifacts; pure module/STIG assertions, real delayed discovery, installed wrapper/config and XCCDF/OpenSCAP probes plus existing developer-tool selftests run directly in one explicit component-tests CI job. No new aggregator framework, registry or public verification output. Existing state-machine-test now owns unique DB regressions,14 integration targets, both populated migration rehearsals and the original broad ignored groups in a fresh invocation-owned PG cluster. Dedicated port/path, LOGIN SUPERUSER CREATEDB, pg_stat_statements and both DB URLs are verified. Child status receipts prevent process-compose success from hiding test failure.

Measured counts at this iteration: checks1->1; verify public outputs15->0; all public package attributes43->28; expanded CI jobs14->9. Eighteen old verification/helper derivations removed; broader recursive recipe union including successor direct-CI tools/templates7563->7547 (net16). The narrower core+five adapter slice5428->4432 is not a whole-pipeline metric. Six duplicate ignored tests,94 repeated normal-library selections and2 repeated composite-policy cases are removed. Normal core/embedded feature configurations still have ordinary package tests; debug DB tests need distinct real PostgreSQL and selected integration targets.

Coverage map now inventories current HTTP/UI/configuration/agent/builder/cache/evidence/import/export/upgrade surfaces against accepted sources:79 obligation rows across12 families. Five rows have executed subsets (23Access cases),65 planned,9 decision-blocked. These are not endpoint percentages or full row completion. Many discovered operations need assignment to existing packets before implementation, not another task family.

Implemented next milestone: .3 Admin UI forms appended after the original16cases in the same Access VM; seven new cases passed at c254b1c54e0f40adfaa9d05b5766cfeffae9bb89. No extra VM derivation. .3 remains In Progress for environment CRUD/nonempty assignments and exact missing failed-save states. .7 shared roles and initial real Git->evaluation->build proof remains the next Delivery foundation; no blanket D1 or old foundation-failure blocker. Six planned scenario boundaries remain the starting topology; do not create one VM per packet or placeholder owners.

Remaining consolidation: CVE/dashboard remain separate existing DB workflows; compare any additional exact-selection overlap and compilation-cache reuse before final minimality. CA compatibility has three pinned provenance inputs currently yielding identical bytes; further content-deduplication may remove repeated input work. Do not claim complete deduplication or total runtime speedup. Final canonical merged DB command is running after fixture/environment fixes; full pass requires its actual status, not passing chunks or a green process supervisor. No assertion weakened or production behavior changed.

Nine legacy suite directories and obsolete manual jobs remain retired per explicit source-removal authority. Missing accepted integration obligations remain gaps. No product/UI/schema/module-behavior changes, merge/deployment or Done claim; .25/.26 prerequisite repairs remain the prior exception.
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

Database worker scope: exclusively own packages/devScripts/default.nix and new packages/devScripts/tests database shell scripts; remove packages/verify-server-regressions only. Preserve six broad state-machine ignored filters, all 14 integration targets, both populated migration rehearsals, encryption fixture, one-case mock config and exact missing-test guards. Independently audit six duplicate ignored selections and 94 normal-lib selections before removing them. Use fresh disposable dedicated PostgreSQL path/port with LOGIN SUPERUSER CREATEDB and pg_stat_statements; export both database URLs in sqlx shell and share Cargo target compilation. Verify shell/Nix syntax and actual program evaluation, selection preservation and minimal real isolated DB smoke. Stage only owned files; no CI/docs edits or commits. Coordinator handles runtime assertion integration requests.

Remaining-eight delegated scope: remove owned verify outputs; preserve pure builder/STIG module assertions as explicit boolean evaluations under modules tests, package metadata assertions in nixos-options-metadata and OSCAL schema validation in oscal-fixture checkPhase. Preserve discovery as explicit isolated runtime command and packaging wrapper/evaluator/runtime-module assertions as explicit commands without replacement package derivations. Audit normal Rust tests; provide exact coordinator CI/devScripts build snippets. Stage only owned paths, no commit.

Delegated five-package slice in existing locked TASK-490.1 worktree: inspect actual Rust assertions; remove verify-config-inspector/config-observer/evaluator-snapshot-isolation/verified-source-evaluator-parity/niks3-ca-bundle public definitions. Preserve unique real Nix protocol tests as cf-server core-only postCheck fragments under existing component tests, retain locked source inputs, and move Go/Rust CA dataset compatibility into core check execution. Select cf-config normal library tests once in core (current cf-server-only selection does not run dependency tests), removing separate CA Rust runner. Evaluate component drvPaths and real dependency inventory; validate through Nix with persistent logs for expensive builds. Stage only this slice; no commit, CI/docs/devScripts or production changes.

Additional assigned XCCDF slice: exclusively delete verify-xccdf-schema definition/README and orphan verify-server-regressions README; add cf-server/tests/xccdf-schema.sh taking installed core, XCCDF schema directory and CF schema file. Retain actual writer-generated XCCDF/OpenSCAP validation and all five CF extension node types with non-vacuous extraction. Remove handwritten example documents and structural duplicates after writer/parser Rust audit. Verify actual installed fixture binary if available; no default.nix edits, stage only assigned paths.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Current clean-slate iteration

Baseline2c34186e, same dedicated TASK-490.1 worktree/branch, draft MR !335; active openai coordinator. Lower-level consolidation7cceb904, Admin forms595b6089, product inventory91e8c5af, runtime-temp fix24f82d4, environment isolationca417334, final assertions/status propagationf10e77ac. Tested Access sourcec254b1c54e0f40adfaa9d05b5766cfeffae9bb89. No task completion/merge/deploy.

All15verify outputs absent; no replacement public package per old suite. Exact primary homes/table are in flake-checks.md and coverage map. Core ordinary build mi1inp9dp6lqx9wq9s2ba40pc2qs1pfy passed status0, including cf-server nonignored tests,94cf-config tests (13CA), actual Nix producer/protocol assertions and Go/Rust DER comparisons. Metadata and OSCAL ordinary builds, all3pure module predicates, delayed authenticated Nix discovery, full installed-wrapper/config probes, actual writer XCCDF/CF-schema/OpenSCAP validation, tool selftests and CI cache-policy/routing tests passed. GitLab lint valid with9expanded jobs. Normal Rust tests and source definitions unchanged. No runtime total-speedup claim.

Counts: check attrs1->1; verify attrs15->0; total public package attrs43->28. Expanded jobs14->9 (six-package matrix replaced by one explicit component-tests job);3existing DB jobs remain. Root/helper recipes removed18; full comparison union old15wrappers+7ordinary roots vs successors plus direct-CI tools/module contexts7563->7547 net16. Narrow five-adapter slice5428->4432 is only a scoped figure. No generic aggregator framework/registry/fifthcheck.

DB selection audit matched compiled inventory: old regression333library executions+51state-machine executions =384; after233+51 =284unique ignored library tests. Removes6ignored overlaps and94normal-library selections already owned by package tests.14integration targets/231cases remain;2duplicate composite-policy executions removed from CVE workflow. Both populated upgrade rehearsals preserved. Dedicated PG35433 with fresh invocation-owned data, superuser/createdb and pg_stat_statements verified; fake occupied-port and wrong-data probes fail closed. Config/env fixtures now use correct DB fields and NUL-delimited env enumeration (Nix Bash has no compgen). Real worker case passed1test in36.79s;116remaining exact cases and51broad group tests passed in isolated follow-up; earlier prefix passed all its integration targets including130POAM cases. These are passing chunks, NOT a full normal command pass. Final canonical run /tmp/opencode/task490-db-final.log, result /tmp/opencode/task490-db-final.status remains to inspect. Do not mark AC#3 complete without full evidence. Child-status receipt exercise: child101+compose0 ->101; success0 ->0; missing receipt ->1; child0+compose7 ->7. Nothing is allowed to hide a test failure.

Product inventory79rows in12families accounts actual registered HTTP families/methods, auth/gates, UI routes/forms/drawers/navigation, config/module roles, Git/evaluation, builders, agents, caches, CVE/hardening/compliance/STIG/remediation, imports/exports/history and upgrades. Five rows have executed subsets,65planned,9decisionblocked. Actor/start/action/independent expectation/observation/status recorded. Code locates mechanisms, not acceptance authority. Exact unresolved decisions: environment scope, closed registration, old release/client compatibility, specified failed-save states, remaining webhook/CSRF policies, draft interchange semantics and missing visual references. No blanket blocker on accepted controller/Delivery work.

Normal Access CI path `sh packages/ci/integration-owner-build.sh integration-access` exited0 at clean c254b1c5;23cases passed and original16 unchanged first. Seven Admin cases have UI reload plus SQL: create name/identity/role, edit, cancel, disable/login401/no session, retry save, banner enable/save. VM executed newly; driver50.72s, startup10.5s shared once; configured4GiB/4cores, peak notmeasured. Two local VM runs during form development (first retry-label locator failed after20passed); corrected locator keeps all persistence/status assertions. Prior300s interrupted package build launched noVM. One registered scenario, not an extra VM per form/browser phase. Evidence /nix/store/nv2rpc4szk7525aa9fpb4289nz1hpbqv-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run plus exported integration-evidence/. Result upload https://gitlab.com/-/project/70402481/uploads/4edf40060b019ca9dbec691d6759ec11/result.json .

Pending: final canonical DB status; any additional CVE/dashboard overlap/cache reuse; identical CA inputs content dedup; .3 environments and states; .7 roles/actualbuild; remaining owners/coverage and final4-name acceptance. Currentcheck inventory still["integration-access"]. .2/.3/.24 and parent remain In Progress. LOCK: openai on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation.

## Historical
The nine legacy suites were source-retired0105e512. One-for-one17lower-level relocation was an interim rejected architecture, not final retention. All15verify packages are now eliminated. .25/.26 prior authorized config repairs enabled bootstrap; sessions and Operator/Viewer cases had earlier passing16-case evidence. TASK-492/493/494 remain unrelated; no product fix was made here.
<!-- SECTION:NOTES:END -->
