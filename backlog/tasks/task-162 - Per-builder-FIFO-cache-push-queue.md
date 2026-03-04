---
id: TASK-162
title: Per-builder FIFO cache push queue
status: Backlog
assignee: []
created_date: '2026-03-03 15:23'
labels:
  - enhancement
  - cache
  - builder
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Currently, the cache push system uses a shared global queue where any cache worker can pick up any pending cache push job. This means:

1. A builder that completes a build may not be the one that pushes its artifacts to cache
2. No guarantee of FIFO ordering per-builder (the job a builder just created might get picked up by a different worker)
3. Less efficient use of bandwidth - a builder might be idle while waiting for another builder's cache push to complete

## Desired Outcome

Each builder instance should have its own FIFO cache push queue:

- When a builder completes a build successfully, it enqueues a cache push job to its own queue
- The same builder instance processes its queue in FIFO order
- This ensures locality (builder pushes what it built), FIFO ordering, and better throughput

## Impact Areas

- `packages/default/src/builder/worker.rs` - build success handling
- `packages/default/src/builder/cache_worker.rs` - cache worker logic
- `packages/default/src/queries/cache_push.rs` - queue selection queries

## Architecture Notes

- Option A: Add `builder_id` column to `cache_push_jobs` and partition claims by builder
- Option B: Keep cache push logic entirely within builder process (no separate worker)
- Option C: Hybrid - builder enqueues to its own named queue, dedicated workers pick up by builder_id

## Risk Level

Medium - requires changes to cache job claim logic and potential schema updates
<!-- SECTION:DESCRIPTION:END -->
