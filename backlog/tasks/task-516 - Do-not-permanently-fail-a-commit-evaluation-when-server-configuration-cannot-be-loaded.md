---
id: TASK-516
title: >-
  Do not permanently fail a commit evaluation when server configuration cannot
  be loaded
status: Backlog
assignee: []
created_date: '2026-11-02T15:59:00Z'
labels:
  - evaluation
  - server
  - reliability
dependencies: []
priority: high
type: bug
ordinal: 567000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`server/mod.rs` (post-claim evaluation path) calls `CrystalForgeConfig::load()` per evaluation. On failure it calls `mark_commit_evaluation_failed` with `RetryFailureClass::Deterministic`, so a transient or environmental config read failure (unreadable or mid-rewrite `config.toml`, bad `CRYSTAL_FORGE__*` value) permanently fails that commit. The failed head then has no declared-output snapshot, and the Add System form cannot discover configurations until a new commit evaluates. Observed on the TASK-508 test deployment (stored error `loading configuration`). TASK-508 only made the stored and logged error include the full cause chain. Artifact hydration and other per-request `CrystalForgeConfig::load()` callers have the same exposure. Not introduced by TASK-508.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Server configuration load failure during evaluation is classified as an infrastructure (retryable) failure, not a deterministic evaluation failure
- [ ] #2 Evaluation uses the configuration loaded at startup, or a reload failure retains the last good configuration, per an explicit decision
- [ ] #3 A test proves a config load failure does not permanently fail the commit
<!-- AC:END -->
