---
id: TASK-440.3
title: Complete scanning rebuild fallback and latest-attempt summaries
status: Backlog
assignee: []
created_date: '2026-09-18 03:51'
labels:
  - scanning
  - cve
  - builds
  - follow-up
  - TASK-440
dependencies: []
references:
  - TASK-440
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/323'
documentation:
  - docs/multi-builder-architecture.md
  - docs/backend-api.md
modified_files:
  - packages/default/crates/cf-server/src/builder/cve_worker.rs
  - packages/default/crates/cf-server/src/models/evaluate_with_policies.rs
  - packages/default/crates/cf-server/src/queries/flakes.rs
  - packages/default/crates/cf-server/src/queries/scanning.rs
  - packages/web-ui/src/views/scanning.rs
parent_task_id: TASK-440
priority: high
type: enhancement
ordinal: 476000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-440 added distributed CVE scanning, rescan actions, diagnostics, and scan-policy controls. Review found that the persisted rebuild-to-scan setting does not yet cause an unmaterialized target to become scannable, and some build summaries can use historical attempts instead of the latest attempt for a derivation. Operators therefore can select a policy that has no effect and can see stale build state around scan workflows.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 When rebuild-to-scan is enabled and a scan target cannot be materialized through authorized cache or local-store paths, the server schedules or reuses the supported build lifecycle required to make that exact target scannable.
- [ ] #2 When rebuild-to-scan is disabled, an unmaterialized scan remains in an explicit terminal or retryable state and does not silently enqueue build work.
- [ ] #3 Build and flake summaries select the latest attempt for each derivation and do not let an older successful or failed attempt override a newer attempt.
- [ ] #4 Scan and build retries remain idempotent and preserve independent build and scan outcomes, authorization, builder-session fencing, and immutable target identity.
- [ ] #5 The UI and API report truthful pending, building, scanning, failed, and completed states throughout the fallback lifecycle.
- [ ] #6 Focused migrated-database, concurrency, backend, and frontend tests cover retry races, latest-attempt selection, and both policy settings.
- [ ] #7 Scanning and multi-builder documentation describe the rebuild fallback, trust boundaries, retry behavior, and operator-visible failure states.
<!-- AC:END -->
