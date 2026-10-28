---
type: Data Model
title: "Deployment Timeline View - Developer Notes"
description: "Developer notes for view_commit_deployment_timeline: the failed derivation_path join, the first-seen-after-commit deployment approximation, limitations, alternatives considered, recommendations, usage notes, and monitoring queries."
tags:
  - crystal-forge
  - deployment-timeline
  - system-states
  - developer-notes
  - monitoring-queries
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:55:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/dev-notes.md at commit 3b23d36f"
    title: "Developer notes (dev-notes.md)"
---
# Deployment Timeline View - Developer Notes

> **Status:** The statement that Crystal Forge does not record deployment events predates `system_events`. Migration `packages/default/crates/cf-server/migrations/0155_system_events_timeline.sql` creates `system_events` and `pending_system_deployments`; see [Authoritative system_events timeline](system-events-timeline.md). This migration found no `deployment_events` table (the hypothetical table in "Alternative Approaches Considered") in `packages/default/crates/cf-server/migrations/` or `packages/default/crates/cf-server/src/`. This migration did not read the current view definition.

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


## Related concepts

- [Commit Deployment Timeline View](views/view-commit-deployment-timeline.md)
- [Authoritative system_events timeline](system-events-timeline.md)
- [Systems Status View - Technical Notes](systems-status-view-technical-notes.md)
