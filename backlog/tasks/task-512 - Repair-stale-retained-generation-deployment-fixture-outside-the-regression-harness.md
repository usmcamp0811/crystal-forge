---
id: TASK-512
title: >-
  Repair stale retained-generation deployment fixture outside the regression
  harness
status: Backlog
assignee: []
created_date: '2026-11-01T15:39:00Z'
updated_date: '2026-11-04T04:20:00Z'
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

Third instance, verified at the base commit 18b47eed in a scratch worktree: queries::cves::tests::newest_attempt_keeps_completed_evidence_after_failed_queued_and_scanning fails there with the same assertion as on the TASK-508 branch (left: Exact, right: MappedRunning at cves.rs). It is #[ignore] and not selected by packages/devScripts/tests/database-regressions.sh, so CI never runs it. It predates TASK-508 (added by TASK-326.2). Not caused by TASK-508 or its 0325/0326 migrations.

Fourth batch, verified at base commit 18b47eed in a scratch worktree (same four failures there as on the TASK-508 branch; none is selected by database-regressions.sh): queries::systems::tests::cve_dedup_migration_counts_distinct_per_severity_correctly and cve_dedup_migration_counts_each_cve_id_once_despite_package_fanout (replaying migration 0111 into a fully migrated schema fails with 'cannot drop columns from view'), hotfix_system_views_cve_counts_from_view_system_vulnerabilities ('cannot create temporary relation in non-temporary schema'), and stale_manual_target_clear_does_not_delete_concurrent_replacement ('operator does not exist: uuid = text'). Not caused by TASK-508.

2026-11-04: two more pre-existing failures found while running the full queries::builders::tests family against a disposable migrated database. queries::builders::tests::test_concurrent_claims_take_top_two_without_duplicates and queries::builders::tests::test_dashboard_queue_matches_next_claim_order_for_queued_items fail the same way at base 18b47eed (verified in a scratch worktree) and at 967b7ea5. Neither runs in the CI harness. Fourteen other tests in that family fail only in a shared run: eleven need a role named postgres, and three ordering tests pass in isolation once earlier failures stop leaving jobs in the shared database. Not caused by TASK-508.
<!-- SECTION:NOTES:END -->
