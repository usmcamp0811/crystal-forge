# Buildable Derivations View (`view_buildable_derivations`)

## Overview

`view_buildable_derivations` exposes the **system-aware build queue** for NixOS system and package derivations that are currently eligible for claiming by build workers.
This view implements coordinated parallel building by tracking which derivations are already reserved by workers, calculating system-level progress, and enforcing dependency completion before system builds.

This view serves as the **primary work claiming source** for the Crystal Forge build worker pool, enabling multiple workers to build packages for the same NixOS system in parallel while preventing premature system builds.
It is also optimized for **Grafana dashboards**, enabling operators to monitor real-time worker coordination, queue depth, and build progress across all systems.

## Example Output

Given two NixOS systems with their package dependencies and active workers:

```
Commit A (2024-01-15 14:30:00):
  - server-alpha (nixos, dry-run-complete)
    - firefox-120.0 (package, dry-run-complete) [worker-0 building]
    - chromium-119.0 (package, dry-run-complete) [available]
    - nginx-1.24 (package, build-complete) [done]

Commit B (2024-01-15 10:00:00):
  - server-beta (nixos, dry-run-complete)
    - python-3.11 (package, build-complete) [done]
    - nodejs-20 (package, build-complete) [done]
```

The view returns (ordered by queue position, excluding reserved items):

| id  | derivation_name | derivation_type | build_type | total_packages | completed_packages | cached_packages | active_workers | queue_position |
| --- | --------------- | --------------- | ---------- | -------------- | ------------------ | --------------- | -------------- | -------------- |
| 103 | chromium-119.0  | package         | package    | 3              | 1                  | 1               | 1              | 1              |
| 201 | server-beta     | nixos           | system     | 2              | 2                  | 2               | 0              | 2              |

**Note:** `firefox-120.0` (id=102) does not appear because it's already reserved by worker-0.

## Purpose

- **Worker Coordination:** Workers claim work by querying `SELECT * FROM view_buildable_derivations LIMIT 1`, getting the highest priority unclaimed derivation.
- **Parallel Building:** Multiple workers can simultaneously build different packages for the same NixOS system.
- **Dependency Enforcement:** NixOS systems only appear in the queue when all their package dependencies are built.
- **Cache Awareness:** Optionally respects cache push completion before allowing system builds (for distributed workers).
- **Operations Monitoring:** Visualize queue state, worker distribution, and system progress in Grafana.

## Core Logic

The view calculates system-level progress and determines which derivations are claimable:

1. **System Progress Calculation:**

   - Counts total packages per NixOS system
   - Counts completed packages (status_id = 6)
   - Counts cached packages (status_id = 14)
   - Counts active workers currently building packages for each system

2. **Package Candidates:**

   - Packages with status "dry-run-complete" or "scheduled" (status_id IN (5, 12))
   - Not already reserved in `build_reservations`
   - Attempt count ≤ 5
   - Ordered by commit timestamp DESC, then by system size ASC (smallest systems first)

3. **System Candidates:**

   - NixOS systems with status "dry-run-complete" or "scheduled"
   - Not already reserved
   - **All packages must be built** (`total_packages = completed_packages`)
   - Rust code applies additional cache check if `wait_for_cache_push` config enabled
   - Ordered by commit timestamp DESC, then by system size ASC

4. **Queue Position:**
   - Sequential numbering for monitoring
   - Combined ordering across packages and systems

## Key Fields

| Field                | Description                                                   |
| -------------------- | ------------------------------------------------------------- |
| `id`                 | Derivation ID (for claiming)                                  |
| `derivation_name`    | Name of the derivation                                        |
| `derivation_type`    | `'nixos'` or `'package'`                                      |
| `derivation_path`    | Store path of the derivation                                  |
| `pname`              | Package name (NULL for NixOS systems)                         |
| `version`            | Package version (NULL for NixOS systems)                      |
| `status_id`          | Current build status                                          |
| `nixos_id`           | Parent NixOS system ID                                        |
| `nixos_commit_ts`    | Commit timestamp for priority ordering                        |
| `total_packages`     | Total packages required for this system                       |
| `completed_packages` | Packages with status = BuildComplete (6)                      |
| `cached_packages`    | Packages with status = CachePushed (14)                       |
| `active_workers`     | Number of workers currently building packages for this system |
| `build_type`         | `'package'` or `'system'`                                     |
| `queue_position`     | Sequential position in the queue (for monitoring)             |

### Ordering Summary

| Level | Sort Field        | Direction | Purpose                                    |
| ----- | ----------------- | --------- | ------------------------------------------ |
| 1️⃣    | `nixos_commit_ts` | DESC      | Newest commits first                       |
| 2️⃣    | `total_packages`  | ASC       | Smaller systems first (within same commit) |
| 3️⃣    | `nixos_id`        | ASC       | Group by system                            |
| 4️⃣    | `pname`, `id`     | ASC       | Stable ordering within system              |

## Example Queries

### Claim next work item (worker perspective)

```sql
SELECT id, derivation_name, derivation_type, build_type,
       total_packages, completed_packages, cached_packages
FROM view_buildable_derivations
LIMIT 1;
```

### Show queue depth by commit

```sql
SELECT
  nixos_commit_ts,
  COUNT(*) FILTER (WHERE build_type = 'package') as pending_packages,
  COUNT(*) FILTER (WHERE build_type = 'system') as ready_systems
FROM view_buildable_derivations
GROUP BY nixos_commit_ts
ORDER BY nixos_commit_ts DESC;
```

### Find systems blocked on dependencies

```sql
SELECT DISTINCT
  nixos_id,
  d.derivation_name as system_name,
  sp.total_packages,
  sp.completed_packages,
  (sp.total_packages - sp.completed_packages) as packages_remaining
FROM view_buildable_derivations vbd
JOIN derivations d ON d.id = vbd.nixos_id
WHERE build_type = 'package'
GROUP BY nixos_id, d.derivation_name, sp.total_packages, sp.completed_packages
ORDER BY packages_remaining DESC;
```

### Visualize queue position over time (Grafana)

```sql
SELECT
  NOW() as time,
  queue_position,
  derivation_name,
  build_type,
  CASE
    WHEN completed_packages = total_packages THEN 'Ready for system build'
    ELSE format('%s/%s packages complete', completed_packages, total_packages)
  END as progress
FROM view_buildable_derivations
ORDER BY queue_position;
```

## Operational Context

### Worker Claiming Algorithm

Each worker (tokio task) follows this pattern:

1. Query `view_buildable_derivations LIMIT 1` to get highest priority work
2. Atomically create a reservation in `build_reservations` using FOR UPDATE SKIP LOCKED
3. If successful, build the derivation
4. On completion/failure, delete the reservation
5. Loop back to step 1

This ensures:

- No two workers claim the same derivation
- Workers naturally distribute across systems
- Newest commits are always prioritized
- Systems only build when ready

### Distributed Worker Support

The `cached_packages` field enables safe distributed building:

- **Local-only workers:** Can claim system builds as soon as `completed_packages = total_packages`
- **Distributed workers:** Should wait until `cached_packages = total_packages` to ensure all dependencies are available in shared cache
- Rust code checks `wait_for_cache_push` config to enforce this

### Grafana Dashboard Integration

Operators can visualize:

- Queue depth and position
- Which systems are building vs waiting
- Worker distribution across systems
- Cache push lag (completed but not cached)
- Systems blocked on package dependencies

## Performance Notes

- Filters restrict to minimal working set (status_id IN (5, 12), attempt_count ≤ 5)
- LEFT JOIN with `build_reservations` filters out already-claimed work
- Leverages indexes:
  - `build_reservations(derivation_id)` - UNIQUE constraint enables fast exclusion
  - `build_reservations(nixos_derivation_id)` - Fast worker counting per system
  - `derivation_dependencies(derivation_id, depends_on_id)` - Dependency traversal
  - `derivations(status_id)` - Status filtering
- Window function `ROW_NUMBER()` generates queue_position for monitoring without affecting claim logic
- Optimized for frequent polling (every 5 seconds per idle worker)

## Related Tables and Views

- **`build_reservations`** - Tracks active worker assignments (see `build_reservations.md`)
- **`view_build_queue_status`** - Monitoring view showing system-level progress (see `view_build_queue_status.md`)
- **`derivations`** - Core derivation table
- **`derivation_dependencies`** - Package-to-system relationships
- **`commits`** - Commit metadata for priority ordering

## Migration Notes

This view replaces `view_nixos_derivation_build_queue` with the following enhancements:

- System-aware progress tracking
- Worker reservation filtering
- Cache completion tracking
- Parallel worker support
- Smallest-systems-first ordering within commits
