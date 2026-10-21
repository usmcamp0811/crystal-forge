---
id: TASK-501
title: Fail dedicated database CI jobs when PostgreSQL initialization fails
status: To Do
assignee:
  - '@openai'
created_date: '2026-10-21 14:36'
updated_date: '2026-10-21 23:56'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner explicitly selected TASK-501 in the TASK-490 wrap-up instruction. Authorization includes actual runner, disposable database and CI execution/failure-propagation repairs, preserving declared test selections and explicit dependency preparation/offline execution. Use the existing MR335 integration as base; do not change production database configuration or add another check/wrapper hierarchy. The earlier discovery-only restriction is superseded.
<!-- SECTION:NOTES:END -->
