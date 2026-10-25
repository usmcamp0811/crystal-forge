---
id: TASK-505
title: Collect optional LLVM coverage through existing component and database tests
status: In Progress
assignee:
  - openai
created_date: '2026-10-25 01:12'
updated_date: '2026-10-25 03:28'
labels:
  - ci
  - coverage
  - simplification
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/338'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2985720111'
modified_files:
  - .gitlab-ci.yml
  - docs/agents/verification.md
  - docs/knowledge/testing/flake-checks.md
  - docs/knowledge/testing/integration-test-contract.md
  - docs/knowledge/testing/test-plan.md
  - packages/ci/test_public_cache_build.py
  - packages/coverage/default.nix
  - packages/coverage/instrument.sh
  - packages/coverage/report.sh
  - packages/coverage/test-report.py
  - packages/default/default.nix
  - packages/default/crates/cf-server/tests/oscal_poam_export.rs
  - packages/default/crates/cf-server/tests/register_oscal_snapshot.rs
  - packages/devScripts/default.nix
  - packages/devScripts/tests/database-environment.sh
  - packages/devScripts/tests/database-regressions.sh
priority: high
type: chore
ordinal: 556000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The coverage job repeats workspace tests and implements a second database/test lifecycle solely to measure coverage. Replace that duplication with optional measurement of existing normal test workflows. Audit and preserve tests that currently execute only under Tarpaulin before deleting its orchestration. Keep this narrowly scoped follow-up separate from MR335 and do not delay or modify its merge verification. Target dev after MR335; no deployment, release, or follow-up merge is authorized.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 An audited before/after test inventory assigns every required Rust test to its normal component or isolated database CI owner and proves execution without coverage enabled
- [x] #2 Optional LLVM instrumentation collects profiles through those existing test workflows with unchanged assertions and isolation; production derivations and ordinary developer builds remain uninstrumented
- [x] #3 An artifact-only collector merges profiles and produces a valid useful report and percentage without executing tests or creating a database
- [x] #4 Actual test failure still fails its owning job; measurement/tool failure is distinguishable and missing or invalid measurement is never reported as successful coverage
- [x] #5 Independent Tarpaulin execution and obsolete coverage-only database supervision, receipts, launcher tests and redundant reports are deleted after equivalent ownership is verified
- [x] #6 The four native integration owners remain unchanged and no new flake check, VM, framework, registry or service is added; no coverage threshold is introduced
- [ ] #7 Affected normal/instrumented builds, database paths, cleanup, failure propagation, report scope, configuration and documentation are verified with before/after job/dependency/code-removal evidence
- [x] #8 Open one separate follow-up MR targeting dev; do not merge it without separate authorization
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Preserve MR335 untouched; use separate TASK505 worktree based on its bb50aaea pending head, retarget/reconcile to dev after335. Audit manifests, actual selection scripts and retained40-target evidence before deletion. Identified gaps:41 cf-protocol tests, five nonignored integration targets(21 tests),12 nonignored liveDB library tests; ignored targets were previously unexecuted and are not automatically adopted.
2. Assign protocol and three pure export integration targets to existing core component check with explicit schema tools/paths. Assign the two migration integration targets to existing state-machine selection; add12 missing genuine liveDB cases to existing CVE owner. Preserve all bodies/assertions, existing environments, guards and cleanup. Do not introduce new test owners/jobs.
3. Add a small shared native instrumentation/export hook reused by opt-in component derivation variants and the existing database environment. Compile with instrument-coverage and common source remapping; isolate instrumented Cargo targets. Export profiles and coverage-bearing executable objects outside disposable DB directories. Ordinary production/default dev definitions remain uninstrumented; toolchain matches pinned rustc LLVM. Enabled CI selects instrumented variants in the same component job, never a separate test runner.
4. Replace packages/coverage with an artifact-only native llvm-profdata merge/llvm-cov report collector. One report/percentage, no Cargo, migrations, database lifecycle, custom HTML/Markdown/JUnit or Tarpaulin. Remove coverage-only launcher controls and obsolete wiring. Coverage-enabled test failures remain owner failures; missing/invalid measurement produces an explicit tool failure, no arbitrary percentage gate. Optional collector job runs only when CF_COVERAGE=1 and consumes normal job artifacts.
5. Verify baseline and changed actual selections/counts, normal disabled ownership, instrumented same owners and portable report validity/failure behavior, normal production drv flags/identity, database cleanup, CI lint/four root checks/full-flake, documentation and deleted-line/dependency/job comparisons. Preserve normal offline test boundaries and no global offline policy. Commit/push one narrow follow-up and open MR to dev, dependent on335 if still unmerged; no merge/deploy.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: openai on local host in /home/mcamp/code/crystal-forge/TASK-505-native-coverage.

## Scope and preflight

- Branch: TASK-505-native-coverage. Base: MR !335 head bb50aaead49f2e91833db0e2b5283da1cbca0c24. Target: dev after !335.
- Risk: medium-high. Missing test ownership or false-green measurement is the main risk. Product APIs, schema, authorization, deployment behavior, and the four integration definitions are not changed.
- The dev worktree has pre-existing untracked locks and session-ses_f927.md. These files remain untouched. No main integration worktree exists. The new worktree was clean before implementation.
- MR !335 remains separate. A read-only check found pipeline 2985383461 failed only coverage-check job 17442644001. No !335 source, gate, status, merge, preview, or process was changed.

## Ownership audit and implementation

Former Tarpaulin execution had gaps in normal ownership: 41 protocol tests; three pure export targets with 13 tests; two migration targets with eight tests; and 12 live-database library tests. Core now owns protocol and pure exports. State-machine owns the two migration targets. CVE owns the 12 live-database cases. Assertions remain unchanged. Two test-only schema helpers accept an explicit Nix schema directory with the original repository-relative fallback.

Eight entirely ignored targets contained 27 tests that Tarpaulin did not execute. This task does not enable that previously skipped inventory or claim those bodies passed.

Optional component variants reuse the existing build/check phases. The database hook uses the existing launcher's fresh TMPDIR/coverage-target and exports before existing cleanup. Normal production derivations remain uninstrumented; agent, builder, and keygen normal derivation paths match the base. The collector uses only native LLVM tools and shell utilities. It never runs Cargo, tests, migrations, or PostgreSQL. CF_COVERAGE=1 enables CI collection; no percentage threshold exists.

## Verified locally

| Database owner | Normal | Instrumented | Selections in each run |
| --- | --- | --- | --- |
| CVE | 43 passed | 43 passed | 33 |
| State-machine | 561 passed | 561 passed | 245 |
| Dashboard | 4 passed | 4 passed | 2 |

Every run had zero failures and exact expected/emitted membership. All six clusters stopped and removed their owned data/socket directories. Instrumented Cargo targets were removed; profiles and objects survived in external bundles. All five normal component builds and all five instrumented variants passed. Core includes the additional protocol and export selections.

- All 16 existing CVE/dashboard CI fault controls passed with verified cleanup. Native temporary fixtures proved test exit 101 remains authoritative, tooling failure returns 65, collection does not rerun tests, corrupt/missing inputs cannot publish report.txt, and real keygen zero coverage (0/72 lines) is valid.
- The real XCCDF schema fixture, five CF extension payload checks, and OpenSCAP validation passed. Its raw profile merged into a writable core bundle. Fixture main execution count changed from 0 to 1; fixture line coverage changed from 0% to 95.52%.
- The installed collector consumed all eight producer bundles once: 232639 mapped authored backend lines, 87806 missed, 62.26% native line coverage. Scope includes mapped tests/examples/build sources. This is not whole-product coverage or a value comparable with the historical Tarpaulin percentage.
- Knowledge validation passed 58 tests, 215 Mermaid blocks, and 193 concepts. Rust formatting, scoped diff checks, launcher builds, and GitLab CI lint passed.
- Full flake check exited 0 for the recorded bb50aaea-dirty snapshot. Actual receipts: Access 37; Delivery 16; Assurance 54 records/48 names; Upgrade 13. All passed with error=null. Driver runtimes: 133.80/536.48/274.37/617.17 seconds. These are not final committed-head receipts. Later documentation, CI-only, and optional profiling-wrapper changes were checked separately. Current owner outPaths differ, so byte-identical snapshot equivalence is not claimed.

Evidence is retained under /tmp/opencode/task505-*, including final-report/report.txt, final-collector.log/status, DB normal/instrumented logs and cleanup receipts, core-schema verification, native-hook fault evidence, dependency-summary.json, and flake.log/status.

## Before and after

- Coverage package: 562 to 101 lines; 461 net lines removed.
- Collector runtime closure: 223 to 22 store paths. Cargo, rustc, Tarpaulin, PostgreSQL, and process-compose are absent from the new collector.
- MR job selection: 11 before; 10 by default; 11 with optional artifact-only collection.
- Four root integration checks remain. checks/, lib/, modules/, shells/, flake.nix, and flake.lock have no task changes. No new check, VM, test owner, registry, or service was added.
- Required offline database execution remains. No global offline setting was introduced. SQLx metadata preparation is not applicable because no queries or schema changed.

## Limits and remaining verification

LLVM emits a visible warning about 2400 mismatched function records in the combined report. A bounded core diagnostic identified dependency mappings requesting hash zero; no inspected CF-authored mismatch was found. This does not prove every combined mapping is correct. No warning suppression, custom LLVM format, or extraction scheme was adopted.

Full ELF coverage objects are retained. The core bundle is about 931 MB, or 303 MB with ordinary ZIP deflation. Actual GitLab profile-artifact upload and the new committed-head pipeline remain unverified under AC7. Keep the follow-up Draft until its dependency and required verification are resolved. Do not use local coverage to waive a failing normal job or !335's gates.

Historical attempts are retained in logs: a ShellCheck runtime-source diagnostic was corrected; collector positional-object/path mapping mistakes were corrected; an absent XCCDF schema output was realized before a successful rerun; a 200-second foreground build wait timed out while the persistent build continued and later passed. Optional ELF-layout experiments did not change repository code; one unsupported layout variant crashed LLVM and was not adopted.

Next: commit the frozen task scope, push its separate branch, open a Draft follow-up MR targeting dev with !335 dependency explicit, and inspect its actual pipeline. No merge, deployment, release, branch deletion, or worktree removal is authorized.

Final consumer audit found packages/ci/test_public_cache_build.py still assumed a single component command and the old coverage preparation/test runner. Updated that existing probe only: all five normal and optional component routes retain exact cache/resource/redaction assertions; injected failures at every loop position propagate immediately. Deleted obsolete coverage preparation expectations; complexity fault controls remain. The full existing routing test passed (exit 0), including genuine Nix fixture fallback; log /tmp/opencode/task505-ci-routing-1.log. No new test framework or collector launcher self-test was added. Final GitLab CI lint is valid with no errors or warnings. Current collector, knowledge validation and all three launcher package builds passed again.

Committed and pushed 67c7d5eefffdf1e16479b9949b1b6a7510ee266b on TASK-505-native-coverage. Opened Draft MR !338 targeting dev: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/338. Its dependency on unmerged !335 is explicit; the own delta is one commit/17 files above bb50aaea, while GitLab's target comparison temporarily includes the parent's commits. No auto-merge is enabled, and no merge/deployment/release occurred. Worktree is clean.
The single automatic committed-head pipeline is 2985720111. Its actual job inventory contains exactly 10 jobs, all with commit 67c7d5ee; coverage-check is absent with collection disabled. Docs and Access were running, other check jobs pending, complexity created at observation. No duplicate pipeline or retry was launched. Read-only observer task505-hosted-watch uses the existing parameterized evidence collector and retains /tmp/opencode/pipeline-2985720111/. AC7 remains open for final-head hosted evidence and optional profile-artifact transport; task remains In Progress with its lock, not Review/Done. Do not copy these execution-only notes into another source commit merely to refresh status.
<!-- SECTION:NOTES:END -->
