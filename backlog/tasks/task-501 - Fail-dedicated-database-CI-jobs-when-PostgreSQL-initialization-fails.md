---
id: TASK-501
title: Fail dedicated database CI jobs when PostgreSQL initialization fails
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-21 14:36'
updated_date: '2026-10-22 00:29'
labels: []
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17341573575'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17341573576'
  - packages/devScripts/default.nix
  - /tmp/opencode/pipeline-2971039033/report.md
documentation:
  - docs/agents/database-safety.md
  - docs/agents/verification.md
priority: high
type: bug
ordinal: 552000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Dedicated CVE-processing and dashboard-visibility CI jobs reported success without executing their database-dependent tests. At published commit3ec99f7273736e69df9c4918727f8328db1cecb1 in pipeline2971039033, both traces reported initdb cannot be run as root and then Job succeeded. The same gap occurred at checkpoint204a4752. The separate state-machine job genuinely ran525 tests but does not prove every dedicated-job obligation. False-green jobs hide missing regression evidence. This discovery is outside the two narrowly authorized Assurance presentation repairs; record it for human sprint selection rather than changing CI during those repairs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 PostgreSQL initialization failure makes each affected CI job exit nonzero before reporting test success.
- [ ] #2 Each affected job executes its declared database-dependent tests on a root runner using an invocation-owned isolated database without touching persistent developer data.
- [ ] #3 Regression coverage proves failure propagation and the supported successful root-runner path with actual test counts.
- [ ] #4 Verification records and affected workflow documentation distinguish executed tests from setup failures and reused results.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Audit exact CVE/dashboard declared selections against the existing consolidated database runner; preserve ignored/integration/filter/extension obligations without duplicate execution.
2. Reuse invocation-owned PostgreSQL privilege isolation, explicit preparation/offline execution and exit-status handling. Keep unique job requirements unless proven subsumed. Retain direct real CI failure propagation through initialization, readiness, migration, test and result completion.
3. Add bounded regression exercises using existing mechanisms for initialization failure, child-test failure, absent completion, interruption and stale/empty selection. No new flake check or generic framework.
4. Run actual intended tests on isolated local/root-compatible invocation storage and record exact names/counts/source/commands/exits. Keep production/developer databases untouched.
5. Review focused runner/CI/docs changes, integrate into existing MR335, and inspect current-head hosted logs/artifacts before claiming the former false-green obligations passed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner explicitly selected TASK-501 in the TASK-490 wrap-up instruction. Authorization includes actual runner, disposable database and CI execution/failure-propagation repairs, preserving declared test selections and explicit dependency preparation/offline execution. Use the existing MR335 integration as base; do not change production database configuration or add another check/wrapper hierarchy. The earlier discovery-only restriction is superseded.

Preflight: dedicated worktree/branch /home/mcamp/code/crystal-forge/TASK-501-truthful-database-ci at clean dbf50b4746a6ed3818aed85af68f26251f4b9d52, based on current MR335 integration head. TASK-501.lock added under canonical backlog ownership. Integration owner worktree is clean and matches remote; origin/dev remains5ae7feab. No main worktree registered. dev contains pre-existing untracked task locks/session file; preserved. Non-UI repair requires no competing preview stack. Existing integration previews remain coordinator-owned and must not be touched by the repair worker.

Selection/implementation decision: retain all three CI jobs. CVE's workspace-wide --lib filters and three --ignored scanning filters, and dashboard's two --ignored visibility prefixes are not fully subsumed by database-regressions.sh/state-machine-tests.sh. Reuse the existing state-machine module/launcher/status receipt with suite-specific commands and dedicated ports (35432 CVE, 35433 state-machine, 35434 dashboard). Preserve original Cargo filters/modes. Common Cargo guard rejects zero executed tests without a second execution. Root verification uses an isolated user namespace (unshare --user --map-auto --map-root-user), with PostgreSQL alone setpriv to nobody; no sudo or preview services required.

BLOCKED: actual root-namespace verification stopped at an existing Rust assertion, per explicit stop-on-new-production-defect instruction. Command in /tmp/opencode/task501-actual.sh: unshare --user --map-auto --map-root-user nix run --offline --option substituters '' --option substitute false --option builders '' --option fallback false --option accept-flake-config false --max-jobs 0 --no-update-lock-file .#devScripts.cve-test -- -t=false up; CARGO_NET_OFFLINE=true SQLX_OFFLINE=true CARGO_BUILD_JOBS=2. Actual outer exit101 (/tmp/opencode/task501-root-cve-test.status and task501-actual.status). Fresh root-compatible initdb succeeded with owner nobody; database migrated through version0310. Exactly one real Rust test passed (builder::cve_worker::tests::scan_cycle_processes_target_with_fake_runner); second real test failed (builder::cve_worker::tests::recorded_derivation_path_restores_when_output_deriver_is_unknown), panic at packages/default/crates/cf-server/src/builder/cve_worker.rs:2705: calls.load(Ordering::SeqCst) expected1, observed0. Remaining CVE selections and dashboard did not execute. Raw /tmp/opencode/task501-root-cve-test.log lines405-449 and shutdown851-857 retain exact evidence. Source Rust/assertions unchanged. No production repair attempted.

Worktree changes remain UNCOMMITTED and not fully verified. All three final launcher builds passed (Nix writeShellApplication shell checks included); explicit nix develop #sqlx cargo fetch --locked passed (/tmp/opencode/task501-prepare.log/status). A task-owned copy of the coordinator compilation cache was reused; test programs actually executed, no result substitution. Compiled-test/source selection audit (/tmp/opencode/task501-selection-audit.json, generated by task501-audit.py in Nix Python) confirms unchanged CVE27 filters selecting30 tests (27 nonignored +3 ignored), dashboard2 ignored filters selecting4 tests. Names/counts in audit are selection inventory, NOT passing execution results. New directly-needed database-ci-test.py exercises the actual CI offline script with temporary compose fault inputs, but is NOT YET EXECUTED. Remaining: root/nonroot successful dedicated jobs, initialization/Cargo/empty-selection/completion/interruption injections, changed shared state-machine execution, CI/docs updates and full verification decision. No push/MR/merge or full-flake/VM runs. Coordinator must resolve the uncovered assertion or authorize further independent harness work before resuming.

User authorizes continued bounded independent runner verification despite uncovered Rust assertion, plus committing coherent WIP runner/CI/regression/docs changes without Review. Diagnose assertion read-only; preserve all production Rust/assertions. Execute controlled actual-entry faults and normal dashboard root/nonroot where feasible. Remaining CVE filters may execute separately for diagnosis; full CVE job must remain failing. Coordinator owns coverage map/contract. No push/MR/merge or full-flake/VM runs.
<!-- SECTION:NOTES:END -->
