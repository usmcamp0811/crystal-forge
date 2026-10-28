---
id: TASK-455
title: Fix ignored evaluation retry test to honor configured backoff
status: Backlog
assignee: []
created_date: '2026-09-04 18:39'
labels:
  - server
  - tests
  - postgresql
dependencies: []
references:
  - packages/default/crates/cf-server/src/queries/commits.rs
modified_files:
  - packages/default/crates/cf-server/src/queries/commits.rs
priority: medium
type: bug
ordinal: 464000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The ignored `queries::commits::tests::failed_attempt_retry_increments_count` regression starts the automatically queued second attempt immediately. The current default retry policy assigns a future `available_at`, so `mark_commit_evaluation_started` correctly returns `NoLongerPending`. Update the regression to control or advance the retry availability without weakening production backoff behavior. Discovered while verifying TASK-440; this is outside TASK-440 scope.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The ignored retry-attempt regression passes against an isolated migrated PostgreSQL database
- [ ] #2 The test preserves verification that attempt numbers increase monotonically
- [ ] #3 Production automatic retry backoff behavior is unchanged
<!-- AC:END -->
