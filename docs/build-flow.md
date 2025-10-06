# Crystal Forge Build Queue Design

## Overview

Crystal Forge builds NixOS systems and their package dependencies. The build system prioritizes completing one system at a time, building all dependencies before the system itself.

## Core Principle

**Build systems one at a time, newest commits first**

Given multiple commits for the same hostname, always prioritize the latest commit. Older commits are ignored unless they are the latest for their hostname.

## Build Priority Order

For each hostname (e.g., `reckless`, `webb`, `chesty`):

1. Build all **packages** needed by the hostname's **newest** commit
2. Build the **NixOS configuration** for that newest commit
3. Build all **packages** needed by the hostname's **second-newest** commit
4. Build the **NixOS configuration** for that second-newest commit
5. Continue through all commits for this hostname (newest → oldest)
6. Move to next hostname (alphabetically)
7. Repeat

### Example Execution

Given systems: `reckless` (5 commits), `webb` (3 commits), `chesty` (4 commits)

```
Priority 1:  All packages for reckless commit #5 (newest)
Priority 2:  reckless NixOS config commit #5
Priority 3:  All packages for reckless commit #4
Priority 4:  reckless NixOS config commit #4
Priority 5:  All packages for reckless commit #3
Priority 6:  reckless NixOS config commit #3
Priority 7:  All packages for reckless commit #2
Priority 8:  reckless NixOS config commit #2
Priority 9:  All packages for reckless commit #1 (oldest)
Priority 10: reckless NixOS config commit #1

Priority 11: All packages for webb commit #3 (newest)
Priority 12: webb NixOS config commit #3
Priority 13: All packages for webb commit #2
Priority 14: webb NixOS config commit #2
Priority 15: All packages for webb commit #1 (oldest)
Priority 16: webb NixOS config commit #1

Priority 17: All packages for chesty commit #4 (newest)
Priority 18: chesty NixOS config commit #4
... continues through all chesty commits
```

**All commits are built**, but prioritized newest-first within each hostname.

## Database Query Logic

The build queue query (`get_derivations_ready_for_build`) returns an ordered list:

### Query Structure

1. **Group by hostname**: All derivations for the same hostname stay together
2. **Order commits within hostname**: Newest to oldest by commit timestamp
3. **Order derivations within commit**: Packages before NixOS configs
4. **Order results**:
   - Primary: hostname (alphabetically)
   - Secondary: commit timestamp (descending - newest first)
   - Tertiary: type_priority (0=package, 1=nixos)
   - Quaternary: ID (stable ordering)

### Key Query Elements

```sql
-- Build CTE of all NixOS derivations with commit info
WITH all_nixos_commits AS (
    SELECT 
        d.id as nixos_deriv_id,
        d.derivation_name as hostname,
        c.commit_timestamp,
        c.id as commit_id
    FROM derivations d
    INNER JOIN commits c ON d.commit_id = c.id
    WHERE d.derivation_type = 'nixos'
      AND d.status_id IN (5, 7)  -- dry-run-complete or build-pending
)

SELECT d.*
FROM derivations d
-- Join packages to their parent NixOS configs
LEFT JOIN derivation_dependencies dd ON dd.depends_on_id = d.id
LEFT JOIN all_nixos_commits n ON dd.derivation_id = n.nixos_deriv_id
-- Join NixOS configs to themselves for ordering info
LEFT JOIN all_nixos_commits anc ON d.id = anc.nixos_deriv_id
WHERE d.status_id IN (5, 7)
  AND (
      -- Include packages that are dependencies of any NixOS config
      (d.derivation_type = 'package' AND n.hostname IS NOT NULL)
      OR
      -- Include all NixOS configs that are ready
      (d.derivation_type = 'nixos' AND anc.hostname IS NOT NULL)
  )
ORDER BY 
    -- Group by hostname (packages get parent's hostname, NixOS uses own)
    CASE 
        WHEN d.derivation_type = 'package' THEN COALESCE(n.hostname, 'zzz_orphan')
        WHEN d.derivation_type = 'nixos' THEN anc.hostname
    END ASC,
    -- Within hostname: newest commits first
    CASE 
        WHEN d.derivation_type = 'package' THEN n.commit_timestamp
        WHEN d.derivation_type = 'nixos' THEN anc.commit_timestamp
    END DESC NULLS LAST,
    -- Within commit: packages before NixOS
    CASE 
        WHEN d.derivation_type = 'package' THEN 0
        WHEN d.derivation_type = 'nixos' THEN 1
    END ASC,
    -- Stable ordering
    d.id ASC
```

## Build Execution

### Concurrent Processing

The build loop processes multiple derivations simultaneously:

1. Query database for ordered build queue
2. Take first N items (where N = `max_concurrent_derivations`, typically 6-8)
3. Spawn concurrent tokio tasks for each derivation
4. Each task:
   - Marks derivation as `build-in-progress`
   - Executes build using `nix-store --realise`
   - Updates status to `build-complete` or `build-failed`
5. Wait for all tasks to complete
6. Repeat

### Resource Limits

Each build runs in a systemd scope with limits:
- Memory: 16-20GB per build
- CPU: 800-1000% (8-10 cores per build)
- Enforced by cgroups to prevent resource contention

## Derivation States

### Status Flow
```
dry-run-complete (5) → build-pending (7) → build-in-progress (8) → build-complete (10)
                                                                  → build-failed (12)
```

### Key States
- **dry-run-complete**: Configuration validated, dependencies discovered
- **build-pending**: Ready to build, waiting for worker
- **build-in-progress**: Currently building
- **build-complete**: Successfully built
- **build-failed**: Build failed (will retry up to 5 times)

## Dependency Discovery

Packages are discovered during the NixOS dry-run evaluation phase:

1. NixOS config completes `nix build --dry-run`
2. Output contains all transitive dependencies (.drv paths)
3. These are inserted as package derivations
4. `derivation_dependencies` table links packages to their parent NixOS config

**Important**: Packages do NOT have `commit_id` set. They are linked to NixOS configs only via `derivation_dependencies` table.

## Configuration Parameters

### Build Config
- `max_concurrent_derivations`: Number of simultaneous builds (default: 8)
- `max_jobs`: Nix build parallelism per derivation (default: 6)
- `cores`: CPU cores available per build job (default: 12-16)
- `poll_interval`: How often to check for new work (default: 30s)

### Resource Limits
- `systemd_memory_max`: Memory limit per build (e.g., "16G")
- `systemd_cpu_quota`: CPU quota as percentage (e.g., 800 = 8 cores)

## What This Design Prevents

1. **Building newest commits last**: Latest deployable versions are always prioritized
2. **Interleaved system builds**: Complete all commits for one system before starting another
3. **Resource contention**: Systemd cgroups enforce hard limits per build
4. **Deadlocks**: Packages always build before their dependent NixOS configs

## What This Design Enables

1. **Fast deployability**: Newest commits build first, making latest versions immediately deployable
2. **Complete history**: All commits eventually build (newest → oldest)
3. **Predictable completion**: One system at a time, one commit at a time means clear progress
4. **Parallel execution**: Within one commit, multiple packages build concurrently

## Future Improvements

### Continuous Worker Pool
Replace timer-based polling with continuous worker tasks:
- Workers continuously claim work from queue
- No artificial delays between batches
- Always maintain configured concurrency level
- Better resource utilization when builds complete at different rates
- See GitLab issue: "Replace Poll-Based Build Loop with Continuous Worker Pool"
