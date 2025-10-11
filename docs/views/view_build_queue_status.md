# Build Queue Status View (`view_build_queue_status`)

## Overview

`view_build_queue_status` provides **system-level monitoring** of the build queue, aggregating progress for each NixOS system including package completion counts, active worker assignments, and cache push status.

This view is designed specifically for **Grafana dashboards and operational monitoring**, enabling Site Administrators to understand queue health, identify bottlenecks, detect stale workers, and track build progress across all systems at a glance.

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

- **Queue Health Monitoring:** Understand build progress across all systems at once
- **Worker Distribution:** See how many workers are assigned to each system
- **Bottleneck Detection:** Identify systems with many packages pending or slow cache pushes
- **Stale Worker Detection:** Alert on workers that have stopped sending heartbeats
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

### Queue depth by commit (Grafana time series)

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

## Operational Context

### Grafana Dashboard Panels

**Panel 1: Systems Overview Table**
- Show all systems with progress bars
- Color code by status (green=ready, yellow=building, red=stale)
- Sort by commit timestamp DESC

**Panel 2: Worker Distribution**
- Bar chart showing active workers per system
- Identify systems with too few/many workers

**Panel 3: Queue Depth Over Time**
- Time series of total pending packages
- Helps identify if queue is growing or shrinking

**Panel 4: Stale Worker Alerts**
- Table showing only systems with `has_stale_workers = true`
- Alert threshold: >5 minutes since heartbeat

**Panel 5: Cache Push Lag**
- Gauge showing systems waiting for cache pushes
- Important for distributed worker setups

### Alerting Rules

**Alert: Stale Workers Detected**
```sql
SELECT COUNT(*) FROM view_build_queue_status WHERE has_stale_workers = true
```
Trigger: count > 0 for >10 minutes

**Alert: Queue Growing Unchecked**
```sql
SELECT SUM(pending_packages + building_packages) FROM view_build_queue_status
```
Trigger: sum > 500 packages pending

**Alert: System Stuck Building**
```sql
SELECT * FROM view_build_queue_status 
WHERE status = 'building' 
  AND earliest_reservation < NOW() - INTERVAL '2 hours'
```
Trigger: any system building for >2 hours

## Performance Notes

- Aggregates across multiple tables but filtered to active work only
- Uses LEFT JOINs to include systems with zero active workers
- Window functions avoided - straight aggregation for speed
- Indexes leveraged:
  - `derivations(derivation_type, status_id)` - Filter NixOS systems
  - `derivation_dependencies(derivation_id)` - Find packages per system
  - `build_reservations(nixos_derivation_id)` - Count workers per system
  - `build_reservations(heartbeat_at)` - Stale worker detection
- Suitable for 1-second refresh in Grafana (even with 1000+ systems)

## Related Tables and Views

- **`view_buildable_derivations`** - Worker-facing queue for claiming work (see `view_buildable_derivations.md`)
- **`build_reservations`** - Active worker assignments
- **`derivations`** - Core derivation table
- **`derivation_dependencies`** - Package relationships
- **`commits`** - Commit metadata

## Migration Notes

This is a new monitoring view introduced alongside the system-aware build queue. It does not replace any existing views but complements `view_buildable_derivations` by providing aggregated monitoring data rather than individual claimable work items.
