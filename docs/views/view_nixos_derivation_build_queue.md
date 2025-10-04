# NixOS Derivation Build Queue View (`view_nixos_derivation_build_queue`)

## Overview

`view_nixos_derivation_build_queue` provides the **ordered build queue** of NixOS and package derivations currently eligible for building. It serves as the **primary scheduling view** for the Crystal Forge builder loop, determining what derivations should be built next based on commit recency and dependency relationships.

This view is also designed for **Grafana dashboards**, providing operators and Site Administrators (SA) with a real-time visualization of the upcoming build queue—showing which NixOS systems and dependent packages are queued for evaluation or build next.

## Purpose

- **Builder Integration:** Used by the Crystal Forge builder loop to fetch the next derivation to build (`SELECT ... LIMIT 1` from this view).
- **Operations Monitoring:** Visualize the build queue in Grafana to see what will be built next, grouped by NixOS system and commit.
- **Dependency Awareness:** Ensures packages associated with a NixOS build are shown first, maintaining correct build order.

## Core Logic

The view joins derivations, commits, and dependency relationships to produce a structured queue of upcoming builds:

1. Filters derivations to `status_id IN (5, 12)` — typically “ready to build” or “evaluation complete”.
2. Identifies all **NixOS root derivations** and their **dependent packages**.
3. Orders results by:

   - Commit timestamp (**newest first**)
   - NixOS ID grouping (packages + root)
   - Within each group:

     - **Packages first**, followed by their NixOS system derivation

## Key Fields

| Field             | Description                                         |
| ----------------- | --------------------------------------------------- |
| `id`              | Derivation ID                                       |
| `commit_id`       | Associated commit ID                                |
| `derivation_type` | `'nixos'` or `'package'`                            |
| `derivation_name` | Name of the derivation                              |
| `derivation_path` | Store path of the derivation                        |
| `status_id`       | Derivation build status ID (restricted to 5 and 12) |
| `attempt_count`   | Number of previous build attempts                   |
| `nixos_id`        | Parent NixOS derivation ID (group key)              |
| `nixos_commit_ts` | Commit timestamp of the associated NixOS build      |
| `group_order`     | Sort order within group (0 = package, 1 = NixOS)    |

### Ordering Summary

| Level | Sort Field        | Direction | Purpose                    |
| ----- | ----------------- | --------- | -------------------------- |
| 1️⃣    | `nixos_commit_ts` | DESC      | Newest NixOS commits first |
| 2️⃣    | `nixos_id`        | ASC       | Group by NixOS build       |
| 3️⃣    | `group_order`     | ASC       | Packages before NixOS      |
| 4️⃣    | `pname`, `id`     | ASC       | Stable in-group ordering   |

## Example Queries

### Get the next derivation to build

```sql
SELECT id, derivation_name, derivation_type, nixos_id, nixos_commit_ts
FROM view_nixos_derivation_build_queue
ORDER BY nixos_commit_ts DESC, nixos_id, group_order, pname NULLS LAST, id
LIMIT 1;
```

### Show full queue for the newest commit

```sql
SELECT derivation_name, derivation_type, nixos_id, group_order
FROM view_nixos_derivation_build_queue
WHERE nixos_id = (
  SELECT nixos_id
  FROM view_nixos_derivation_build_queue
  ORDER BY nixos_commit_ts DESC
  LIMIT 1
)
ORDER BY group_order, pname;
```

### Visualize upcoming builds (Grafana table)

```sql
SELECT
  nixos_commit_ts AS time,
  derivation_name,
  derivation_type,
  CASE WHEN group_order = 0 THEN 'Package' ELSE 'NixOS' END AS queue_stage
FROM view_nixos_derivation_build_queue
ORDER BY nixos_commit_ts DESC, nixos_id, group_order;
```

## Operational Context

### Builder Loop Usage

The Crystal Forge builder service queries this view to obtain the next derivation to build, ensuring deterministic build ordering:

- Packages dependent on a NixOS build are processed first.
- Once all packages are built, the NixOS system derivation is scheduled.
- This guarantees build consistency and avoids dependency deadlocks.

### Grafana Dashboard Integration

System Admins can visualize:

- Which NixOS builds and packages are queued
- The commit order of upcoming builds
- Real-time updates as derivations complete or are retried

## Performance Notes

- Filters restrict to the minimal working set (`status_id IN (5, 12)`).
- Leverages indices:

  - `derivation_dependencies(derivation_id)`
  - `derivation_dependencies(depends_on_id)`
  - Partial index on `derivations(status_id IN (5, 12))`

- Optimized for frequent polling by the builder service and live Grafana dashboards.

## Related Views

- **`view_derivation_status_breakdown`** – Global summary of all derivation statuses
- **`view_commit_build_status`** – Commit-level aggregation of build results
- **`view_commit_nixos_table`** – Compact NixOS-only per-commit progress
- **`view_system_deployment_status`** – Current deployment position of each system
