---
id: TASK-507
title: >-
  Agent non-Niks3 cache copy failures echo raw Nix stderr into logs and server
  reports
status: Backlog
assignee: []
created_date: '2026-10-25 18:37'
labels:
  - agent
  - cache
  - security
  - diagnostics
dependencies: []
priority: medium
type: bug
ordinal: 558000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Found while fixing TASK-506. In `cf-agent/src/deployment/agent.rs`, `copy_from_cache` (used for Attic, S3, Http and Nix caches that have no authenticated read config) collects every stderr line containing "error" into `error_buffer` and fails with `nix copy failed: {error_buffer}`. That string is logged and sent to the server through `report_deployment_failed`. Nix stderr can contain cache URLs (including query parameters), credential-bearing paths and provider response text.

TASK-506 added a bounded, fixed-category classifier (`classify_read_failure`) for the authenticated Niks3 path only. The remaining path should reuse that classifier instead of echoing raw output, keeping the existing progress logging, timeout and error behavior otherwise unchanged. Do not change which cache types use which path.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Non-authenticated cache copy failures report a fixed category instead of raw stderr
- [ ] #2 Raw Nix stderr never reaches logs or deployment-failed reports on this path
- [ ] #3 A regression test proves a failure containing a URL with a secret query parameter is redacted
<!-- AC:END -->
