---
id: TASK-505
title: Collect optional LLVM coverage through existing component and database tests
status: In Progress
assignee:
  - openai
created_date: '2026-10-25 01:12'
updated_date: '2026-10-25 01:12'
labels:
  - ci
  - coverage
  - simplification
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
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
- [ ] #1 An audited before/after test inventory assigns every required Rust test to its normal component or isolated database CI owner and proves execution without coverage enabled
- [ ] #2 Optional LLVM instrumentation collects profiles through those existing test workflows with unchanged assertions and isolation; production derivations and ordinary developer builds remain uninstrumented
- [ ] #3 An artifact-only collector merges profiles and produces a valid useful report and percentage without executing tests or creating a database
- [ ] #4 Actual test failure still fails its owning job; measurement/tool failure is distinguishable and missing or invalid measurement is never reported as successful coverage
- [ ] #5 Independent Tarpaulin execution and obsolete coverage-only database supervision, receipts, launcher tests and redundant reports are deleted after equivalent ownership is verified
- [ ] #6 The four native integration owners remain unchanged and no new flake check, VM, framework, registry or service is added; no coverage threshold is introduced
- [ ] #7 Affected normal/instrumented builds, database paths, cleanup, failure propagation, report scope, configuration and documentation are verified with before/after job/dependency/code-removal evidence
- [ ] #8 Open one separate follow-up MR targeting dev; do not merge it without separate authorization
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Sprint selection is explicit in the owner's implementation request. Risk: medium/high CI false-green or missing-test risk; changes affect Nix packaging, CI, database test selection and coverage documentation, not product APIs/schema/UI. Non-goals: MR335 redesign/merge interference, production instrumentation, new database lifecycle/framework, coverage floor, owner VM changes, deployment/release. Verify actual before/after selections/counts, normal coverage-disabled builds and DB jobs, optional instrumented execution and native LLVM report, test/tool failure distinctions, cleanup, dependency/job/deleted-line comparison, four-check inventory, CI lint and full flake validation. Implementation base must contain MR335; while it is unmerged use its clean bb50aaea head in a separate branch and mark the follow-up dependent on335 before review. No other active TASK505 lock exists. Integration dev has only the previously reported untracked lock files and session-ses_f927.md; leave untouched. No main integration worktree exists.

LOCK: openai on local host in /home/mcamp/code/crystal-forge/TASK-505-native-coverage. Dedicated clean branch TASK-505-native-coverage created from pending MR335 bb50aaea; remote dev remains511dcdaf. MR335 is open/non-Draft with ci_still_running, no conflicts; leave its source, processes and pipeline untouched. This follow-up targets dev after335 and must not merge automatically.
<!-- SECTION:NOTES:END -->
