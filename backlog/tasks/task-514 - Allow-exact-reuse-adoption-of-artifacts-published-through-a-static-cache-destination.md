---
id: TASK-514
title: >-
  Allow exact-reuse adoption of artifacts published through a static cache
  destination
status: Review
assignee: []
created_date: '2026-11-02T03:35:00Z'
updated_date: '2026-11-03T13:55:00Z'
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
- [x] #1 A decision records whether and how a completed static-destination publication proves eligibility for reuse and reused-head scan admission
- [x] #2 Static-only deployments adopt an exact successful origin job without a duplicate build when the decision permits it, and fail closed otherwise
- [x] #3 Isolated database tests cover static, database and ambiguous legacy publication sources for reuse adoption and scan admission
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Resolved inside TASK-508 (MR !340) on the user's decision of 2026-11-03: implement static reuse as part of TASK-508 and do not defer it.

Decision (AC #1): a completed static publication of the exact output qualifies for reuse admission and reused-head scan admission only when its recorded destination equals the destination of the currently effective static cache configuration (cache.push_to while cache.push_after_build is true). It qualifies nothing when publication is disabled, push_to is missing or blank, the destination changed, or the configuration cannot be loaded. Admission is a boolean and grants no credential or execution authority.

Implementation (AC #2): migration 0328 adds a 4-argument overload of derivation_publication_eligible and leaves the 3-argument database/legacy rule unchanged. cache_push::effective_static_destination applies the same rule as the static job resolver and the scanner's materialization check; reusable_physical_build_tx and admit_system_cve_scan pass the live value, with *_with_static seams for tests.

Downstream checks inspected and tested: a remote scan claim carries no cache source (asserted); the server-local scanner re-resolves static sources against the configuration in force (existing unit tests: push_after_build, exact push_to, no static fallback for legacy); delivery's resolve_publication_read_tx has no static source, so a static publication yields no read source even when a database destination has the same URL (new test).

Tests (AC #3): six PostgreSQL tests (static accepted only for the effective destination; disabled, blank, changed and non-exact refused; failed, pending, in-progress, other-output and other-derivation refused; database and legacy rules unchanged; ambiguous legacy not rescued by static configuration; scan of a reused head refused without a matching static publication; no delivery source) and two unit tests (effective destination rules; admission equals the static job resolver for five configurations). With the static clause disabled, the four static tests fail and the two unchanged-behavior tests pass. Assurance adds a case that re-records the real publication as static (a named injected prerequisite) and exercises the real evaluator adoption without a second build and a manual rescan through the real scanner.

Limits: no real static publication exists in the integration topology, which publishes through a database destination. The Assurance case result at the final commit is recorded in TASK-508.
<!-- SECTION:NOTES:END -->
