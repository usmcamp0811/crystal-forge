---
id: TASK-490.24
title: >-
  Coordinator: four-owner cutover, CI entry point, and legacy integration
  retirement
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:39'
updated_date: '2026-10-19 19:26'
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
Coordinates the four functional integration owners and lower-level verification. Final checks.x86_64-linux must contain exactly integration-access, integration-delivery, integration-assurance and integration-upgrade, with complete accepted coverage and no placeholders. Access and Delivery now exist; Assurance and Upgrade remain absent.

Tests belong to stable product contracts and behavior-oriented scenarios, not tracking tasks. Access uses sessions, authorization and admin-forms modules in one controller/database lifecycle. Delivery uses one five-role source/evaluation/build lifecycle with semantic source-build.py. Explicit native owner aggregates build every registered gate. No plugin framework, registry, generic verification aggregator, task-specific VM or fifth check is permitted.

All fifteen verify-* package outputs and the nine superseded legacy suites remain retired. Unique assertions run in ordinary component builds, direct module/runtime/schema/tool CI and the existing isolated PostgreSQL workflow. Normal Rust tests remain enabled. The earlier consolidation measured packages43->28 and CIjobs14->9; those historical counts preceded the new Delivery owner/job and are not current-head derivation or runtime metrics.

The corrective iteration renamed all six task-numbered Access fragments, documented their actual dependencies and preserved23case IDs/receipts/order. It repaired true offline outer/nested Nix settings, separate controlled preparation, absolute declared Bash launchers, root-runner PostgreSQL privilege separation and explicit XCCDF realization. Full local database execution passed515tests under outbound-denied client networking. Hosted corrective pipelines2961382083 and2961428492 passed all seven pre-Delivery required jobs.

Initial Delivery coverage now executes a real authored Git commit through native evaluation, authoritative drv, authenticated API-only BuilderA realization and signed local publication, plus a separate BuilderB session and real target registration/periodic reporting. Eight mandatory cases passed locally. TASK-495 was separately authorized and implemented in its own worktree after offline=true exposed an unsupported nix-store flag; no test configuration downgrade or xfail was used. TASK-496 remains an observed heartbeat-classification defect.

The existing product inventory has79rows:11with passed subsets,59planned and9decision-blocked. These are not percentages or full-row completion. Known failing runtime variants remain explicit. Final clean-head/hosted Delivery verification must be recorded before claiming the corrective source ready. Remaining Access, Delivery, Assurance and Upgrade obligations stay open. No merge, deploy or Done is authorized.
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
- [x] #12 Coordinator owns shared harness, composition, registration and CI; owner-local case fragments reuse same-scenario receipts only; no root tests/, custom discovery, registry or new test framework
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

Owner-directed corrective iteration at reviewed head45f2b57e in existing locked TASK-490.1 worktree/branch (openai coordinator). Preserve existing topology, IDs, receipt names and production behavior. Rename task-numbered Access fragments to stable sessions/authorization/admin-forms modules; document explicit shared-state prerequisites and bootstrap-first ordering, update active references and behavior ownership/tracking columns. Repair CI execution: separate controlled prerequisite realization from offline state-machine phase; outer/nested Nix disable substituters/builders/fallback and Cargo/SQLx offline; generated launcher uses declared absolute Bash; explicitly realize existing XCCDF schema output. Verify effective settings, network-denied execution if supported, existing 23 Access cases, full isolated database suite and component commands. Commit semantic cleanup and CI repairs separately, push draft!335 and inspect actual required hosted jobs before proceeding with substantive Delivery coverage. No new investigation task, wrapper, registry, production repair, merge or deploy. Delivery continuation uses existing .7 with behavior-named files and shared harness only after corrective verification.

Subsequent explicit owner authorization permits the separate TASK-495 product correction only. It was developed in its own worktree and applied as an independent commit to the coordinator branch. Keep offline=true and every Delivery success/output assertion. This supersedes the earlier no-production-repair instruction only for TASK-495; TASK-496 and other product changes remain out of scope.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Verified code and hosted results

Verified code revision: `bea0c29936c2220a8c1ba8dbfeb5750dacc18cde`.
Hosted pipeline: https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2964502717.
All eight jobs passed: Access, Delivery, component, state-machine, CVE, dashboard, complexity and coverage. Downloaded artifacts confirm Access 23/23 and Delivery 8/8 at that exact source. The hosted database trace totals 515 passed, 0 failed, 0 ignored across 227 summaries, plus both populated migration rehearsals.

Both normal owner commands exited 0 locally at the same clean revision. Access driver: 54.18 s; Delivery driver: 79.32 s. Delivery uses five roles, one controller/database and 10 GiB configured memory; peak memory is unmeasured. Local immutable evidence:
- Access: `/nix/store/798g3ypl22k5m3g7z2x65blwvndw37lc-vm-test-run-cf-it-access-lifecycle-local-bootstrap-run`.
- Delivery: `/nix/store/61x9c3ii0zw0b6c4pzln0af1vcm8zhfd-vm-test-run-cf-it-delivery-delivery-lifecycle-run`.

## Architecture and correction

All six Access fragment paths are behavior-oriented. Case IDs, receipts, assertions and bootstrap/session/authorization/form ordering remain intact. Delivery explicitly registers one source/evaluation/authorized-build lifecycle, not a packet VM or smoke gate. Native aggregates require every registered gate; no .run bypass, registry, DSL or root tests/ directory exists.

CI repairs provide separate controlled preparation, explicit offline outer/nested Nix settings, declared absolute Bash, root PostgreSQL privilege separation, invocation-owned short sockets and explicit existing XCCDF realization. Full local PostgreSQL execution also passed under outbound-denied client networking. The host daemon was outside that namespace; this is not proof of a firewall-isolated daemon.

TASK-495 was separately authorized and implemented in its own worktree as 6844d185, applied as fc0c5836. It is Review in draft MR !335. offline=true and every Delivery assertion remain. TASK-496 and P3/TASK-492 remain explicit failing runtime variants, not fixed by positive reporting/startup subsets.

## Cold preparation limits and history

Source-generator fan-out measured 2,525 to 5 direct dependencies, not a whole-owner reduction. An opaque raw recipe closure worked warm but failed cold export before VM startup at f369aacd. The native baseline now prepares its complete recipe/build closure once for compatible roles; cold hosted Delivery passed. Cold preparation remains a minimization constraint. Pipeline 2964012282 was superseded/canceled, not passed; 2964350322 failed cold preparation. Their evidence is retained.

Earlier consolidation removed all 15 verify outputs and nine legacy suites without disabling normal Rust tests. Historical pre-Delivery measures were packages 43 to 28, expanded jobs 14 to 9 and lower-level recipes 7,563 to 7,547; these are not current whole-pipeline speed/recipe claims. Six ignored overlaps, 94 repeated normal cf-server selections and two duplicate integration calls were removed. The canonical database selection retains 284 unique ignored-library plus 231 integration cases in 14 targets. Core's config selection expanded from 13 CA-only to 94 units; TASK-495 adds two ordinary regressions, and all 96 passed on coordinator source.

## Remaining scope

Current inventory: Access and Delivery; 28 public packages; zero verify outputs. The single 79-row catalog has 11 rows with passed subsets, 59 planned and nine decision-blocked, not percentage/full-row coverage. Assurance/Upgrade are absent. Extended Access/Delivery and final minimization remain open. Final criteria 7/8/10/11 are unchecked; architecture criterion 12 is verified. Keep parent/this task In Progress and MR !335 draft. No merge, deployment or Done.

LOCK: openai on sledge in /home/mcamp/code/crystal-forge/TASK-490.1-integration-foundation.
<!-- SECTION:NOTES:END -->
