---
id: TASK-512
title: >-
  Repair stale retained-generation deployment fixture outside the regression
  harness
status: Backlog
assignee: []
created_date: '2026-11-01T15:39:00Z'
updated_date: '2026-11-02T02:26:00Z'
labels:
  - tests
  - database
dependencies: []
priority: low
type: bug
ordinal: 563000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`queries::evaluation_snapshots::tests::retained_generation_survives_store_metadata_loss_and_blocks_commit_deletion` (ignored SQLx test) inserts `cache_push_jobs` without `store_path`. Deployment target resolution has required `cpj.store_path = d.store_path` since before TASK-508 (asserted by `queries::systems::tests` on the resolver SQL), so `queue_manual_deployment_atomic` fails with "No cached NixOS store path is available". The test is not selected by `packages/devScripts/tests/database-regressions.sh`, so the failure is not visible in CI. Discovered during TASK-508 isolated test runs; not caused by TASK-508.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The fixture records the exact completed cache-push store path required by deployment target resolution
- [ ] #2 The test passes in an isolated migration-backed database without weakening its retained-generation or commit-deletion assertions
- [ ] #3 The test is either added to the isolated regression harness or the reason it is excluded is documented
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Second instance found during TASK-508 review fixes: queries::scanning_tests::waiting_scan_promotes_through_exact_prerequisites (ignored, not in the regression harness) inserts cache_push_jobs without store_path. promote_waiting_cve_scans is byte-identical to dev @18b47eed and requires push.store_path = derivation.store_path, so the concurrent promotion assertion sees 0 promotions. Same stale-fixture class; not caused by TASK-508.
<!-- SECTION:NOTES:END -->
