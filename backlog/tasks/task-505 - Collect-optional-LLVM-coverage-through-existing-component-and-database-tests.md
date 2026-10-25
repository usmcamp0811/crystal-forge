---
id: TASK-505
title: Collect optional LLVM coverage through existing component and database tests
status: To Do
assignee:
  - openai
created_date: '2026-10-25 01:12'
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
