---
type: Concept
title: "Legacy system status determination logic"
description: "Historical definition of how view_systems_status_table determined Up to Date, Outdated, Unknown State, and Offline from dry-run success and latest commit; open it only for the old logic, the current contract is the System Deployment Status View."
tags:
  - crystal-forge
  - system-status
  - system-current-state
  - dry-run
  - legacy
implementation_status: historical
status: deprecated
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:55:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/dev-notes.md at commit 3b23d36f"
    title: "Developer notes (dev-notes.md)"
---
# Systems Status Logic - Developer Documentation

> **Historical notes:** The status rules below describe the older evaluation
> pipeline, not the current deployment-status contract. Migrations 0287–0291
> compare the observed running store path with the newest eligible,
> exactly cache-published NixOS artifact for the registered flake and effective
> system configuration. A dry-run result or newer raw commit alone cannot make
> a running system `behind`. See
> [System Deployment Status View](../data-model/views/view-system-deployment-status.md) for
> the current status definitions and SQL view.

> **Status:** historical. Migrations `packages/default/crates/cf-server/migrations/0287_deployable_system_status.sql` through `0291_prefer_deployable_running_path_identity.sql` exist and define the replacement contract. The current definition of how a system's state is determined from its observed running store path is in [System Deployment Status View](../data-model/views/view-system-deployment-status.md); connectivity state is in [System Heartbeat Status View](../data-model/views/view-system-heartbeat-status.md). This concept keeps the old logic for its reasoning.

## Status Determination Logic

The `view_systems_status_table` determines system status by comparing the most recent **deployable configuration** against the latest available commit.

### Key Principle: Dry-Run Success = Deployable Configuration

Once a derivation reaches `dry-run-complete` status, the system is considered to have that commit's configuration available, **regardless of subsequent build success or failure**.

### Status Progression and Meaning

```
Git Commit → Derivation Evaluation Pipeline:

pending → queued → dry-run-pending → dry-run-in-progress
    ↓
dry-run-complete ✅ ← SYSTEM HAS THIS COMMIT'S CONFIG
    ↓
build-pending ✅ ← Still has the config
    ↓
build-in-progress ✅ ← Still has the config
    ↓
build-complete ✅ ← Config + successful build
    OR
build-failed ✅ ← Config available, build artifacts failed
    ↓
complete ✅ ← Fully processed
```

### Statuses That Count as "System Has This Commit"

```sql
ds.name IN (
    'dry-run-complete',    -- Configuration evaluated and valid
    'build-pending',       -- Queued for build (config ready)
    'build-in-progress',   -- Building (config ready)
    'build-complete',      -- Built successfully
    'build-failed',        -- Build failed BUT config is valid
    'complete'             -- Fully complete
)
```

### Statuses That DON'T Count

- `pending` - Not yet processed
- `queued` - Waiting for evaluation
- `dry-run-pending` - Evaluation not started
- `dry-run-in-progress` - Evaluation in progress
- `dry-run-failed` - Configuration is invalid/broken

## System Status Categories

### 🟢 Up to Date

- **Definition**: System's latest deployable configuration matches the newest available commit
- **Logic**: `latest_deployable_commit_hash = latest_available_commit_hash`
- **Example**: System has `dry-run-complete` for commit `abc123`, and `abc123` is the latest commit

### 🔴 Outdated

- **Definition**: System's latest deployable configuration is behind the newest available commit
- **Logic**: `latest_deployable_commit_hash ≠ latest_available_commit_hash`
- **Example**: System has `build-complete` for commit `abc123`, but newer commit `def456` exists

### 🟤 Unknown State

- **Definition**: System is deployed but has no deployable configurations in recent evaluations
- **Logic**: `system_states.derivation_path IS NOT NULL` AND `no valid derivations found`
- **Causes**:
  - All recent evaluations failed at dry-run stage
  - System deployed with configuration not tracked in derivations table
  - Data inconsistency

### ⚫ Offline

- **Definition**: No recent deployment state recorded
- **Logic**: `system_states.derivation_path IS NULL` OR `no system_states entries`
- **Causes**:
  - Agent not reporting
  - System genuinely offline
  - Network connectivity issues

## Implementation Details

### Latest Deployable Configuration Query

```sql
latest_successful_derivation AS (
    SELECT DISTINCT ON (d.derivation_name)
        d.derivation_name,
        d.commit_id,
        ds.name AS status,
        c.git_commit_hash,
        c.commit_timestamp
    FROM derivations d
    JOIN derivation_statuses ds ON d.status_id = ds.id
    LEFT JOIN commits c ON d.commit_id = c.id
    WHERE d.derivation_type = 'nixos'
        AND ds.name IN (
            'dry-run-complete',
            'build-pending',
            'build-in-progress',
            'build-complete',
            'build-failed',
            'complete'
        )
    ORDER BY d.derivation_name, c.commit_timestamp DESC
)
```

### Key Join Logic

- **System Registration**: `systems.hostname` = `derivations.derivation_name`
- **Latest Available**: Most recent commit in system's flake
- **Current Deployment**: Most recent `system_states` entry per hostname
- **Status Comparison**: Deployable commit vs available commit

## Architectural Context

### Why "Dry-Run Success" Matters

In NixOS/Crystal Forge architecture:

1. **Dry-run evaluation** validates configuration without building
2. **Successful dry-run** means the configuration is syntactically correct and dependencies are resolvable
3. **Build phase** creates actual artifacts but doesn't change configuration validity
4. **Systems can deploy** configurations that had successful dry-runs, even if builds failed

### Data Model Limitations

- **No explicit deployment events**: Status inferred from periodic state snapshots
- **No rollback tracking**: Can't detect when systems revert to older configurations
- **Build vs deployment separation**: Build failures don't prevent configuration deployment

## Common Scenarios

### Scenario 1: Normal Progression

```
Commit A → dry-run-complete → build-pending → build-complete
Status: System is "Up to Date" throughout the process
```

### Scenario 2: Build Failure

```
Commit A → dry-run-complete → build-pending → build-failed
Status: System is still "Up to Date" (configuration is valid and deployable)
```

### Scenario 3: Evaluation Failure

```
Commit A → dry-run-pending → dry-run-failed
Status: System remains at previous commit status (not updated to Commit A)
```

### Scenario 4: Multiple Commits

```
System on Commit A (build-complete)
Commit B → dry-run-complete (newer timestamp)
Status: System is "Up to Date" with Commit B (even though A was fully built)
```

## Troubleshooting

### System Shows "Unknown State"

1. Check if recent derivations exist for the hostname
2. Verify derivation evaluations aren't all failing at dry-run stage
3. Confirm `derivations.derivation_name` matches `systems.hostname`

### System Shows "Outdated" Unexpectedly

1. Check if latest derivation is stuck in non-qualifying status
2. Verify commit timestamps are correct
3. Look for derivations that failed dry-run validation

### System Shows Wrong Status

1. Verify agent is reporting current state
2. Check for hostname mismatches between systems and derivations
3. Confirm flake associations are correct

## Future Improvements


> **Status:** The "Explicit Deployment Tracking" item above is proposed in this document. Migration `packages/default/crates/cf-server/migrations/0155_system_events_timeline.sql` later created `system_events`; see [Authoritative system_events timeline](../data-model/system-events-timeline.md). This migration did not compare the `deployment_events` sketch with that table.
### Explicit Deployment Tracking

Consider adding direct deployment event recording:

```sql
CREATE TABLE deployment_events (
    hostname TEXT,
    commit_id INTEGER,
    deployed_at TIMESTAMP,
    status TEXT
);
```

### Agent Enhancements

- Report current git commit hash in system state
- Include configuration generation metadata
- Track deployment success/failure explicitly

### Status Refinements

- Distinguish between "deployable" and "deployed"
- Add "deployment in progress" status
- Track configuration drift detection


## Related concepts

- [System Deployment Status View (current status contract)](../data-model/views/view-system-deployment-status.md)
- [System Heartbeat Status View](../data-model/views/view-system-heartbeat-status.md)
- [Systems Status View - Technical Notes](../data-model/systems-status-view-technical-notes.md)
- [Agent heartbeat versus full state persistence](../deployment/agent-heartbeat-vs-state-persistence.md)
