---
id: TASK-543
title: >-
  Fake-executable test fixtures intermittently fail with ETXTBSY in cf-server
  unit tests
status: Backlog
assignee: []
created_date: '2026-11-24T18:10:00Z'
labels:
  - testing
dependencies: []
references:
  - >-
    backlog/tasks/task-529 -
    Show-the-real-deployment-failure-reason-in-history-and-logs.md
  - >-
    backlog/tasks/task-491 -
    Server-package-build-intermittently-fails-existing-Niks3-probe-unit-tests.md
priority: medium
type: bug
ordinal: 579000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
cf-server unit tests that write a fake executable and run it at once can fail when a concurrent test thread forks and the child inherits the fixture's write descriptor (exec gives ETXTBSY). TASK-529 notes record six such tests failing across runs in builders.rs, cve_worker.rs and derivations/cache.rs, including on unmodified origin/dev. TASK-542.1 saw it again on 2026-11-24: derivations::cache::publication_signing_tests::selected_publication_key_signs_resolved_root_and_propagates_failure failed with NotFound at cache.rs:638 (the script never ran, so its arguments file was not written). The same code passed in the previous farm run. TASK-491 fixed only the raw-fork regression in builders.rs.

## Desired outcome
The cf-server-core-drv check phase does not fail because of the fixture exec race, for example: write each fake executable to a temporary name and rename it after the descriptor is closed, or retry exec on ETXTBSY only in the test helper.
<!-- SECTION:DESCRIPTION:END -->
