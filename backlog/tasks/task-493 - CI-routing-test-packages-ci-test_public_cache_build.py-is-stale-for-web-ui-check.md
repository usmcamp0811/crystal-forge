---
id: TASK-493
title: >-
  CI routing test packages/ci/test_public_cache_build.py is stale for
  web-ui-check
status: Backlog
assignee: []
created_date: '2026-10-18 15:35'
labels:
  - ci
  - testing
dependencies: []
references:
  - packages/ci/test_public_cache_build.py
  - TASK-490.24
priority: low
type: chore
ordinal: 544000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`nix develop -c python3 packages/ci/test_public_cache_build.py` fails with an AssertionError at the `web-ui-check` argv assertion (around line 187). The test expects `build ... web-ui -L --show-trace --builders '' ...`, but the job runs `--keep-failed` and redirects output to a log. The failure reproduces on the unmodified TASK-490.1 baseline `9c6ce8ff`, so it predates TASK-490.24. No CI job or flake check runs the test, so the drift went unnoticed.

Found while validating the TASK-490.24 early CI transition. The matrix routing loop (all 13 entries) passes before the failing assertion. Not fixed there because it is unrelated to scheduling.

Fix by updating the expectation to the current job, or by wiring the test into an existing check. Do not weaken the cache-policy assertions.
<!-- SECTION:DESCRIPTION:END -->
