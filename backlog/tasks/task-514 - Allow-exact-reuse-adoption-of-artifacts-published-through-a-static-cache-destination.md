---
id: TASK-514
title: >-
  Allow exact-reuse adoption of artifacts published through a static cache
  destination
status: Backlog
assignee: []
created_date: '2026-11-02T03:35:00Z'
labels:
  - builds
  - cache
  - security
dependencies: []
priority: low
type: enhancement
ordinal: 565000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`derivation_publication_eligible` (migration `0322_exact_derivation_reuse.sql`) accepts only `cache_push_jobs` rows with `cache_destination_source` `database`, or unambiguous `legacy`. Rows with source `static` (emitted by `queries/cache_push.rs` when no database destinations exist and static `push_after_build` is configured) never qualify. Effects in a static-only deployment: `reusable_physical_build_tx` does not adopt a successful origin job, so an identical-drv carrier builds again; scoped scan admission of a reused head returns `scan_ineligible`. Behavior fails closed and creates no false evidence. The SQL predicate cannot see the server's current static configuration, so accepting `static` requires a decision on how to bind static provenance to the current `push_to` (compare `builder/cve_worker.rs` static materialization checks). Discovered during TASK-508 review fixes; not observed in the integration topology, which uses a database destination.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A decision records whether and how a completed static-destination publication proves eligibility for reuse and reused-head scan admission
- [ ] #2 Static-only deployments adopt an exact successful origin job without a duplicate build when the decision permits it, and fail closed otherwise
- [ ] #3 Isolated database tests cover static, database and ambiguous legacy publication sources for reuse adoption and scan admission
<!-- AC:END -->
