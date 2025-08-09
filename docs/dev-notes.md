# Systems Status View - Technical Notes

## Problem Statement

The `view_systems_status_table` was incorrectly showing all systems as "Unknown State" despite having successful deployments. The view determines whether systems are running the latest configuration by comparing deployed systems against available commits.

## Root Cause Analysis

### Initial Approach (Broken)

The original view attempted to join `system_states.derivation_path` with `derivations.derivation_path`:

- **Deployed paths** (from `system_states`): `/nix/store/c2caz4arvnsslgygc4lylxj1byy9bd1p-nixos-system-butler-25.05.20250806.077ce82`
- **Derivation paths** (from `derivations`): `/nix/store/kj9l2mqq7laanvm8sryzq6dny2qrs47f-nixos-system-butler-25.05.20250806.077ce82.drv`

**Key Issue**: These paths are fundamentally different:

- The deployed path is the **built result**
- The derivation path is the **build recipe** (`.drv` file)
- They have different store hashes and the derivation path includes `.drv` extension

### Hash Extraction Attempt (Failed)

Attempted to extract commit hashes from deployed paths, but discovered:

- Short hashes in paths (e.g., `077ce82`, `b6bab62`) are **nixpkgs commit hashes**
- Our git commit hashes are different (e.g., `c881116`, `32d71b8`)
- No reliable way to correlate these hashes

## Solution

### Correct Approach

Match systems by **derivation name** and compare the most recent successful evaluation against the latest available commit:

1. **Current State**: Get latest deployment timestamp and system info per hostname
2. **Latest Successful Derivation**: For each system name, find the most recent successful derivation evaluation (`dry-run-complete`, `build-complete`, or `complete`)
3. **Latest Available Commit**: Get the newest commit for each system's flake
4. **Status Logic**:
   - **Offline**: No deployment recorded
   - **Unknown State**: Deployed but no successful derivation found
   - **Up to Date**: Latest successful derivation matches latest commit
   - **Outdated**: Latest successful derivation is behind latest commit

### Key Insight

The relationship is: `systems.hostname` → `derivations.derivation_name` → `commits.git_commit_hash`

This approach works because:

- Derivation names correspond to system hostnames
- We can reliably track which commits have successful evaluations
- We compare evaluation commits against available commits, not deployed paths

## Implementation Notes

- Use `DISTINCT ON` with `ORDER BY timestamp DESC` to get latest records
- Filter derivations to `derivation_type = 'nixos'` and successful statuses
- Handle NULL cases appropriately for offline/unknown systems
- Sort results to show up-to-date systems first

## Database Schema Dependencies

- `system_states`: Current deployment state per hostname
- `derivations`: Build evaluations linked to commits
- `derivation_statuses`: Success/failure status of evaluations
- `commits`: Git commits in flakes
- `systems`: System registration with flake associations
- `flakes`: Repository configurations

## Future Considerations

- Consider adding deployment tracking that directly links `system_states` to `derivations.id`
- Investigate if NixOS provides a way to extract original commit info from deployed paths
- Monitor for cases where derivation names don't match hostnames exactly

# Deployment Timeline View - Developer Notes

## Problem Statement

The `view_commit_deployment_timeline` was showing zero deployments despite having successful evaluations. The view was attempting to correlate commits with system deployments, but the original implementation had fundamental data model misunderstandings.

## Root Cause Analysis

### Original Broken Approach

The view tried to join `derivations.derivation_path` with `system_states.derivation_path`:

```sql
LEFT JOIN public.system_states ss ON d.derivation_path = ss.derivation_path
```

**Problems:**

1. **Path mismatch**: Same issue as in `view_systems_status_table`
   - `derivations.derivation_path`: `.drv` files (build recipes)
   - `system_states.derivation_path`: Built results (different store hashes)
2. **Conceptual flaw**: Assumes we can directly correlate derivation builds to system state

### Data Reality Check

Analysis of `system_states` revealed:

- **Not deployment events**: Continuous state snapshots (hourly/periodic)
- **Volume**: 295 entries over 3 days for `mattis` = ~98 entries/day
- **No atomic deployment tracking**: No direct link between commits and system state changes

## Architecture Understanding

### What Crystal Forge Actually Tracks

1. **Commits**: Git commits in watched flakes
2. **Derivations**: Build evaluations linked to commits
3. **System States**: Periodic snapshots from agents (hostname, timestamp, current derivation_path)
4. **No Direct Deployment Events**: System state changes are inferred, not recorded

### The "Deployment" Inference Problem

Crystal Forge doesn't record deployment events. Instead, we must infer them by:

- Finding successful derivation evaluations for a commit
- Looking for system state entries that appear after the commit timestamp
- Assuming the "first seen after commit" ≈ deployment time

## Solution Approach

### Key Insights

1. **"Deployment" = approximation**: First system state entry after successful evaluation
2. **Current status**: Based on most recent successful derivation per system
3. **Timeline granularity**: Limited by agent reporting frequency

### Implementation Strategy

```sql
-- Step 1: Get evaluation status per commit
WITH commit_evaluations AS (...)

-- Step 2: Find latest successful derivation per system
latest_successful_by_system AS (...)

-- Step 3: Approximate deployment time
system_first_seen_with_commit AS (
    SELECT
        commit_id,
        hostname,
        MIN(timestamp) FILTER (WHERE timestamp > commit_timestamp) AS first_seen_after_commit
    FROM system_states
    WHERE timestamp > derivation_commit_time
)
```

### What the View Now Shows

- **Evaluation metrics**: Total/successful evaluations per commit
- **Deployment approximation**: When systems first reported after commit evaluation
- **Current status**: Which systems are currently running each commit's configuration
- **Timeline bounds**: First/last system seen after commit

## Limitations and Caveats

### Data Quality Issues

1. **Agent connectivity**: Offline systems won't report state changes
2. **Reporting frequency**: Deployment timing accuracy limited by agent intervals
3. **Clock skew**: System clocks vs server clocks may cause timing issues

### Conceptual Limitations

1. **No rollback tracking**: Can't detect when systems revert to older commits
2. **No failed deployment detection**: Assumes successful evaluation = eventual deployment
3. **Configuration drift**: Systems may report state without actual config changes

### Query Performance

- **30-day window**: Limits historical data to maintain performance
- **Multiple CTEs**: Complex query may be slow on large datasets
- **Cross joins**: Approximation logic requires careful indexing

## Alternative Approaches Considered

### Direct Deployment Tracking

**Pros**: Accurate, explicit deployment events
**Cons**: Requires agent changes, database schema updates

```sql
-- Hypothetical deployment_events table
CREATE TABLE deployment_events (
    id UUID PRIMARY KEY,
    hostname TEXT NOT NULL,
    commit_id INTEGER REFERENCES commits(id),
    started_at TIMESTAMP,
    completed_at TIMESTAMP,
    status TEXT -- 'success', 'failed', 'in_progress'
);
```

### Derivation Path Correlation

**Pros**: Uses existing data
**Cons**: Proven unreliable due to store path differences

### Agent-Reported Commit Hash

**Pros**: Direct correlation between system state and git commits
**Cons**: Requires NixOS integration to expose commit hashes to agents

## Recommendations

### Short Term

1. **Use current approximation approach**: Best available with existing data
2. **Monitor data quality**: Watch for systems with missing timeline data
3. **Add alerting**: Flag systems that don't report state after evaluations

### Long Term

1. **Consider explicit deployment tracking**: Add deployment events to data model
2. **Agent improvements**: Report git commit hash in system state
3. **Configuration management**: Track config application success/failure

### Database Optimizations

1. **Index system_states(hostname, timestamp)**: Critical for timeline queries
2. **Partition system_states by date**: Improve query performance over time
3. **Regular cleanup**: Archive old system_states to maintain performance

## Usage Notes

### Expected Behavior

- **Recent commits**: Should show deployment activity within hours
- **Successful evaluations**: Should correlate with system timeline entries
- **Current deployments**: Should match latest successful derivations

### Debugging Steps

1. Check for systems missing from timeline despite successful evaluations
2. Verify agent connectivity for systems showing old deployments
3. Compare evaluation timestamps with first-seen timestamps for timing validation

### Monitoring Queries

```sql
-- Systems missing from timeline despite successful evaluations
SELECT derivation_name
FROM derivations d
JOIN derivation_statuses ds ON d.status_id = ds.id
WHERE ds.is_success = true
  AND derivation_name NOT IN (SELECT hostname FROM system_states);

-- Systems with stale deployment data
SELECT hostname, MAX(timestamp) as last_seen
FROM system_states
GROUP BY hostname
HAVING MAX(timestamp) < NOW() - INTERVAL '2 hours';
```
