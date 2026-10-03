---
id: TASK-477
title: >-
  Repair three evaluation_snapshots PostgreSQL tests that fail outside CI
  coverage
status: Backlog
assignee: []
created_date: '2026-10-03 18:24'
labels:
  - server
  - tests
  - evaluation-snapshots
dependencies: []
modified_files:
  - packages/default/crates/cf-server/src/queries/evaluation_snapshots.rs
  - checks/server-regressions/default.nix
priority: medium
type: bug
ordinal: 498000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Three ignored PostgreSQL tests in queries/evaluation_snapshots.rs fail on a clean PostgreSQL 17.11 database at the current MR head, and also fail on the unmodified committed version of the file, so they are pre-existing. The flake check named server-regressions does not run them, so no pipeline reports them. They are failed_and_corrupt_snapshots_requeue_with_active_lifecycle (an assertion expects Unavailable but gets Available), missing_v2_lifecycle_ignores_obsolete_inspection_target (a foreign key violation on config_inspection_jobs), and retained_generation_survives_store_metadata_loss_and_blocks_commit_deletion ("No cached NixOS store path is available for deployment target"). The last message suggests the fixtures predate the stricter deployable-artifact rules introduced by migrations 0287 to 0291. Each should be classified as fixture drift or a product defect before changing anything.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 For each of the three tests, the root cause is established as fixture drift or a product defect, with the evidence recorded.
- [ ] #2 Each test passes against an isolated PostgreSQL database, and a product defect is fixed in product code rather than by loosening the assertion.
- [ ] #3 Each test is either added to the server-regressions check or its absence from the check is documented with the reason.
- [ ] #4 No applied migration is edited.
<!-- AC:END -->
