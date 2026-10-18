---
id: TASK-490.24
title: >-
  Coordinator: four-owner cutover, CI entry point, and legacy integration
  retirement
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:39'
updated_date: '2026-10-18 21:05'
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
Coordinator-owned final four-owner completion and minimal lower-level testing. Final checks.x86_64-linux contains exactly FOUR TOTAL native Snowfall owners: integration-access, integration-delivery, integration-assurance, integration-upgrade. Currently only Access exists. Four attributes is not four test commands.

The preservation-first relocation is rejected: all fifteen packages/verify-* wrappers are eliminated, not renamed or grouped behind a generic output. Unique assertions execute in ordinary core/metadata/OSCAL component builds, direct module/runtime/schema/tool CI commands, and the existing consolidated isolated state-machine database workflow. No new aggregator framework, registry or fifth check. Normal Rust tests remain; important security, transaction, concurrency, integrity and protocol protection remains.

Measured counts: check attrs1->1; verify attrs15->0; public package attrs43->28; expanded CI jobs14->9. Eighteen former gate/helper recipes removed; broader lower-level recipe union including required direct-CI tools/generated module contexts7563->7547 (net16). Narrow core+five adapters5428->4432 is not a whole-pipeline metric. Remove6ignored overlaps,94repeated cf-server normal-library selections and2composite-policy invocations. Ordinary cf-config testing expands from13CA-only toall94config tests; gross102redundant selections removed is not a102net whole-CI test-count reduction. Core/embedded feature variants still run normal package tests.

Full canonical consolidated DB command passed at c254b1c54e0f40adfaa9d05b5766cfeffae9bb89: nix run .#devScripts.state-machine-test -- -t=false up exited0.227Rust summaries total515passed/0failed/0ignored:284selected ignored-library cases and231integration cases across14targets. Both populated migration rehearsals passed. One fresh invocation-owned PostgreSQL cluster, dedicated35433/path, superuser/createdb, pg_stat_statements, both DB URLs, one Cargo target directory. Runtime18m26s. Cluster shut down and cleaned. Independent child-status receipt prevents process-compose success from hiding failure. Fixture config/env parsing issues fixed in runner only, no test weakened.

Coverage map inventories actual current HTTP/UI/configuration/Git/builder/agent/cache/evidence/interchange/upgrade surfaces against accepted authorities:79rows/12families,5with executed subsets (23Access cases),65planned,9decisionblocked. These are not endpoint percentages or full row completion. Discovered operations need assignment within existing packets before implementation, not another task family.

Runnable product progress: .3 Admin UI forms appended after original16cases in same local Access scenario. Seven new cases passed at c254b1c5 with new VM, independent SQL and UI reload. No additional VM derivation. Environment CRUD/nonempty assignments and specified missing failure states remain open. .7 is next real service/builder/target role and Git->evaluation->actualbuild proof; no blanket D1 or old foundation failure blocker. Delivery/Assurance/Upgrade remain absent, not placeholders. Keep six initial scenario boundaries and minimize justified isolation.

Remaining minimality work: existing CVE/dashboard workflows still have separate contexts/cache paths; inspect further overlap before consolidation. Three pinned CA provenance inputs currently yield identical bytes and can be content-deduplicated. Batch exact Rust invocations only after auditing process-global state and the six discovery tests requiring independent processes. No total runtime speedup or perfect minimality claim.

Nine legacy suites/manual jobs stay retired under explicit owner authority. Missing accepted integration obligations stay gaps. No production/UI/API/schema/design/module-behavior changes, merge/deploy/Done. Prior .25/.26 repairs remain the old exception.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Automatic integration-access builds its normal owner aggregate through the existing cache wrapper/resource limits and propagates failures; never use a .run output as the gate
- [x] #2 Source retirement removes checks/integration, oidc-auth, niks3-cache, niks3-write-probe, niks3-ui, attic-metrics, attic-publication, web-ui and web-ui-reconciliation after external consumers are relocated; obsolete legacy CI jobs and dead asset references are removed
- [x] #3 Lower-level Rust/database/package/schema/documentation/tool verification has explicit working package or CI execution paths outside flake checks; preserve isolated PostgreSQL and normal package tests
- [x] #4 Owner evidence exports real dereferenced files with artifacts always; failed-build collection processes unterminated last records, preserves the original status after copy errors, never rebuilds .run, and a successful build with missing evidence fails
- [x] #5 Contract, retirement map, verification guide and task records distinguish executed coverage, required gaps, obsolete suites, and lower-level verification retained outside checks; retirement does not mark missing obligations covered
- [x] #6 Evaluate the actual flake inventory and changed package/import/CI consumers; run retained tooling/schema/doc checks and normal Access CI execution, recording revision, result and evidence reuse/execution
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
## Current verified iteration

Source tested c254b1c54e0f40adfaa9d05b5766cfeffae9bb89 in existing TASK-490.1 worktree/branch; draft !335. Task remains In Progress because final owners/coverage/minimality are incomplete. Active coordinator openai. Code consolidation7cceb904; Admin forms595b6089; inventory91e8c5af; workspace hygiene24f82d4; environment isolationca417334; corrected form assertions, env enumeration and status propagationf10e77ac. Documentation/task-only followups do not require another VM.

All15verify public outputs absent. Primary ownership/table: docs/knowledge/testing/flake-checks.md. Core ordinary build mi1inp9dp6lqx9wq9s2ba40pc2qs1pfy passed with cf-server tests,94cf-config tests (13CA), real producer/evaluator/protocol assertions and Go/Rust DER checks. Metadata/OSCAL ordinary builds,3pure module predicates, independent-store delayed Nix discovery, full installed-wrapper/generated config probes, real writer XCCDF/CF-schema/OpenSCAP validation, tool selftests, CI routing/cache policy and GitLab lint all passed. No production Rust test definition deleted, doCheck=false, new ignored assertion or required allow_failure.

Counts measured: check1->1; verify15->0; total publicpackages43->28; expanded CIjobs14->9. Old15wrappers+7ordinary roots lower-level recursive recipe union7563 vs7547successors including direct-CI tools/template contexts (net16). Narrow five-adapter/core union5428->4432 is only scoped. Eighteen former gate/helper recipes removed; data helpers remain real fixture inputs, not exported gates. No aggregator framework or registry.

Compiled selection audit and full execution: old333regression-library executions+51broad=384; after233+51=284unique ignored-library tests.94removed selections were repeated cf-server normal-library tests, different from94cf-config unit cases now selected by core. Six ignored overlaps and two repeated composite-policy cases also removed. Config testing expands13CA-only->94units, so do not claim102net total test reduction across CI/feature/coverage runs.

FULL canonical DB path passed, not merely chunks: nix run .#devScripts.state-machine-test -- -t=false up exited0 at c254b1c5.227Rust summaries sum515passed,0failed,0ignored;284library+231integration cases/14targets, both populated migration rehearsals. UTC20:39:03->20:57:29 (18m26s). Evidence /tmp/opencode/task490-db-final.log and .status=0; database shut down/cleaned. Fresh invocation-owned path/PG35433 with LOGIN SUPERUSER CREATEDB and pg_stat_statements validated. Both URLvariables set insideSQLxshell; sharedtarget/database-tests. Prior failing attempts exposed missing runner-config DB fields and invalid inherited watched map; Nix Bash lackscompgen, so replaced silent process-substitution cleanup with env-0. Fixed worker1test36.79s and independenttail116+51passed before full run. Failure propagation exercised: child101/compose0->101, child0/compose0->0, missingreceipt->1, child0/compose7->7. No failure hidden or production fixed.

Full product map79obligations/12families covers registered method families, UI interactions, configuration, Git/evaluation, builders, agents, caches, scans/evidence/remediation, interchange/history/upgrades with actor/start/action/independent result/observation/status.5rows have executed subsets,65planned,9specific decision-blocked. Code discovers mechanisms; acceptance comes from approved sources. Unassigned discovered cases must be attached to existing packets before implementation.

Normal Access CI command exited0 at c254b1c5:23/23cases, original16first unchanged;7new Admin phases withSQL/reload. Positive named account/role, edit, cancel, disable/login401/no session, retry and banner saves. VM executed; successfuldriver50.72s, startup10.5s reusedonce; configured4GiB/4cores, peak unmeasured. Two localVMruns whiledeveloping forms; first exact-label locator failed after20passes, prefix fix retained statuses/state/reload assertions. Prior300s interrupted package compilation launched noVM; metadata artifactidentity requiredone-time embedded rebuild. Evidence /nix/store/nv2rpc4szk7525aa9fpb4289nz1hpbqv-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run; uploaded result /uploads/4edf40060b019ca9dbec691d6759ec11/result.json.

Pending: final three owners/coverage and four-name acceptance; .3environment CRUD/nonempty assignments/failure states; .7real roles/build proof; further CVE/dashboard overlap/cache reuse; identical CA input content dedup; possible safe native-libtest batching while preserving process-global isolation. .2/.3/.24 and parent stay In Progress. No merge/deploy/Done.

LOCK: openai on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation.

## Historical
Nine legacy suites were removed0105e512. One-for-one17lower-level relocation was rejected, not final retention; all15verify outputs removed now. .25/.26 prior configuration repairs enabled bootstrap; sessions/role denials earlier proved16cases. TASK-492/493/494 remain separate. Previous manual-coexistence, source-preservation and pending-DB notes are superseded by this full verified result.
<!-- SECTION:NOTES:END -->
