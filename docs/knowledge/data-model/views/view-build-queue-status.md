---
type: Data Model
title: "Build Queue Status View (`view_build_queue_status`)"
description: "Describes view_build_queue_status, the SQL view that aggregates package progress, reservation-based worker counts, stale workers, and cache lag per NixOS system for the legacy build-reservation queue, which the server no longer starts; the live build queue is build_jobs."
tags:
  - crystal-forge
  - view
  - build-queue
  - workers
  - legacy
implementation_status: historical
status: deprecated
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/views/view_build_queue_status.md at commit 3b23d36f"
    title: "Build Queue Status View (`view_build_queue_status`)"
---
# Build Queue Status View (`view_build_queue_status`)

## Overview

`view_build_queue_status` provides **system-level monitoring** of the build queue, aggregating progress for each NixOS system including package completion counts, active worker assignments, and cache push status.

## Status

> **Legacy queue.** The view still exists in the schema (defined by migration `0057_parallel_build_queue.sql`). It describes the legacy **build-reservation** queue: workers claim derivations through `build_reservations` rows. The server does not start that worker path. `run_build_loop` has no caller in the server startup code, and the Rust code that reads this view lives in `cf-server/src/queries/build_reservations.rs`, which only the legacy `builder/worker.rs` path calls. The live build queue is `build_jobs`, claimed by API-only builders. See [Wakeups and polling](../../architecture/event-driven-queues.md#build-queue). With no reservations, `active_workers` is 0 and `has_stale_workers` is false for every system.
>
> Other consumers: the database tests (`packages/cf-test-suite/cf_test/tests/database/test_view_build_queue_status.py`, `tests/builder/test_reservation_build_queue.py`). The Dioxus UI and the server API do not read this view.

## Example Output

Given three NixOS systems at various stages of building:

```
System A (Commit 2024-01-15 14:30:00):
  - 10 total packages
  - 7 completed, 2 building, 1 pending
  - 2 active workers (worker-0, worker-1)
  - 5 packages pushed to cache

System B (Commit 2024-01-15 10:00:00):
  - 3 total packages
  - 3 completed, 0 building, 0 pending
  - 0 active workers
  - 3 packages pushed to cache
  - Ready for system build

System C (Commit 2024-01-14 09:00:00):
  - 50 total packages
  - 45 completed, 3 building, 2 pending
  - 3 active workers
  - Last heartbeat: 10 minutes ago (STALE!)
```

The view returns:

| system_name | commit_timestamp    | total_packages | completed_packages | building_packages | pending_packages | cached_packages | active_workers | status                  | cache_status            | has_stale_workers |
| ----------- | ------------------- | -------------- | ------------------ | ----------------- | ---------------- | --------------- | -------------- | ----------------------- | ----------------------- | ----------------- |
| server-a    | 2024-01-15 14:30:00 | 10             | 7                  | 2                 | 1                | 5               | 2              | building                | NULL                    | false             |
| server-b    | 2024-01-15 10:00:00 | 3              | 3                  | 0                 | 0                | 3               | 0              | ready_for_system_build  | NULL                    | false             |
| server-c    | 2024-01-14 09:00:00 | 50             | 45                 | 3                 | 2                | 40              | 3              | building                | waiting_for_cache_push  | true              |

## Purpose

- **Queue Health Monitoring:** Understand reservation-based build progress across all systems at once
- **Worker Distribution:** See how many workers are assigned to each system
- **Bottleneck Detection:** Identify systems with many packages pending or slow cache pushes
- **Stale Worker Detection:** Identify workers that have stopped sending heartbeats
- **Capacity Planning:** Track queue depth and worker utilization over time
- **Cache Lag Monitoring:** See when systems are built but waiting for cache pushes

## Core Logic

The view aggregates data from multiple tables to create a system-centric view:

1. **Joins:**
   - `derivations` (NixOS systems) with `commits` for timestamp ordering
   - `derivation_dependencies` to find all packages for each system
   - `build_reservations` to count active workers and track heartbeats

2. **Aggregations per system:**
   - Count total packages
   - Count packages by status (completed, building, pending)
   - Count packages pushed to cache
   - List active worker IDs
   - Find earliest reservation and latest heartbeat

3. **Derived Fields:**
   - `status`: Current build phase (`pending`, `building`, `ready_for_system_build`)
   - `cache_status`: Whether waiting for cache pushes
   - `has_stale_workers`: Workers with heartbeat >5 minutes old

4. **Filtering:**
   - Only NixOS systems with status "dry-run-complete" or "scheduled"

## Key Fields

| Field                | Description                                                    |
| -------------------- | -------------------------------------------------------------- |
| `nixos_id`           | Derivation ID of the NixOS system                              |
| `system_name`        | Name of the NixOS system (hostname)                            |
| `commit_timestamp`   | When this system's commit was made                             |
| `git_commit_hash`    | Git commit hash for this system                                |
| `total_packages`     | Total package dependencies for this system                     |
| `completed_packages` | Packages with status = BuildComplete (6)                       |
| `building_packages`  | Packages with status = BuildInProgress (8)                     |
| `pending_packages`   | Packages not yet built or being built                          |
| `cached_packages`    | Packages with status = CachePushed (14)                        |
| `active_workers`     | Number of workers currently building packages for this system  |
| `worker_ids`         | Array of worker IDs assigned to this system                    |
| `earliest_reservation` | When the first worker claimed work for this system           |
| `latest_heartbeat`   | Most recent heartbeat from any worker on this system           |
| `status`             | Current phase: `pending`, `building`, or `ready_for_system_build` |
| `cache_status`       | `waiting_for_cache_push` if built but not fully cached, else NULL |
| `has_stale_workers`  | TRUE if any worker heartbeat is >5 minutes old                 |

## Example Queries

### Show systems with the most work remaining

```sql
SELECT 
  system_name,
  pending_packages + building_packages as work_remaining,
  active_workers,
  commit_timestamp
FROM view_build_queue_status
WHERE status != 'ready_for_system_build'
ORDER BY work_remaining DESC
LIMIT 10;
```

### Find stale workers requiring intervention

```sql
SELECT 
  system_name,
  worker_ids,
  latest_heartbeat,
  NOW() - latest_heartbeat as time_since_heartbeat,
  building_packages
FROM view_build_queue_status
WHERE has_stale_workers = true
ORDER BY latest_heartbeat ASC;
```

### Monitor cache push lag

```sql
SELECT 
  system_name,
  completed_packages,
  cached_packages,
  (completed_packages - cached_packages) as cache_lag,
  cache_status
FROM view_build_queue_status
WHERE completed_packages > cached_packages
ORDER BY cache_lag DESC;
```

### Worker utilization summary

```sql
SELECT 
  SUM(active_workers) as total_active_workers,
  COUNT(*) as systems_in_queue,
  COUNT(*) FILTER (WHERE status = 'building') as systems_building,
  COUNT(*) FILTER (WHERE status = 'ready_for_system_build') as systems_ready
FROM view_build_queue_status;
```

### Queue depth by commit

```sql
SELECT 
  commit_timestamp as time,
  COUNT(*) as systems,
  SUM(pending_packages + building_packages) as total_work_remaining,
  SUM(active_workers) as workers_assigned
FROM view_build_queue_status
GROUP BY commit_timestamp
ORDER BY commit_timestamp DESC;
```

## Related Tables and Views

- **`view_buildable_derivations`** - Reservation-queue source of claimable work ([Buildable Derivations View](view-buildable-derivations.md))
- **`build_reservations`** - Legacy worker assignments
- **`derivations`** - Core derivation table
- **`derivation_dependencies`** - Package relationships
- **`commits`** - Commit metadata

## Related concepts

- [Buildable Derivations View](view-buildable-derivations.md)
