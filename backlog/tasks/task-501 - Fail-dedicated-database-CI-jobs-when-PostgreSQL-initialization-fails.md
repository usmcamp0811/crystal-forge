---
id: TASK-501
title: Fail dedicated database CI jobs when PostgreSQL initialization fails
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-21 14:36'
updated_date: '2026-10-22 00:21'
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
<!-- SECTION:NOTES:END -->
