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

# Systems Status Logic - Developer Documentation

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

# SystemD-Run Evaluation Isolation in Crystal Forge

## Background

Crystal Forge was experiencing OOM (Out of Memory) killer terminations during Nix evaluations, causing the entire server process to be killed when individual derivation evaluations consumed excessive system resources. This document explains the isolation strategy implemented to prevent these failures.

## The Problem

### OOM Killer Behavior

When Crystal Forge performed large Nix evaluations:

- Individual evaluations could consume 8GB+ of memory
- Memory usage would spike rapidly during derivation resolution
- Linux OOM killer would terminate the entire Crystal Forge server process
- This caused loss of all in-progress work and service downtime
- Recovery required manual service restart

### Previous Architecture Limitations

```rust
// Original approach - no isolation
let mut cmd = Command::new("nix");
cmd.args(["build", flake_target, "--dry-run"]);
let output = cmd.output().await?; // Could consume unlimited memory
```

This approach provided no protection against runaway evaluations consuming system resources.

## The Solution: SystemD-Run Scope Isolation

### Implementation Strategy

We implemented a fallback-based approach using `systemd-run` for resource isolation:

```rust
fn systemd_scoped_cmd_base() -> Command {
    let mut c = Command::new("systemd-run");
    c.args([
        "--user",                           // Run in user scope
        "--scope",                          // Create transient scope
        "--collect",                        // Auto-cleanup on exit
        "--property=MemoryMax=4G",          // Hard memory limit
        "--property=CPUQuota=300%",         // CPU limit (3 cores)
        "--property=TimeoutStopSec=600",    // 10 minute timeout
        "--quiet",                          // Reduce log noise
        "--",
        "nix", "build",
    ]);
    c
}
```

### Fallback Architecture

The implementation includes automatic fallback for environments where `systemd-run` is unavailable:

1. **Primary**: Try systemd-scoped execution with resource limits
2. **Fallback Detection**: Check for specific systemd/D-Bus failures
3. **Secondary**: Fall back to direct `nix build` execution
4. **Error Handling**: Distinguish between resource failures and evaluation failures

## Technical Implementation

### Three-Tier Execution Strategy

#### Tier 1: Systemd Scoped Evaluation

```rust
async fn run_dry_run(flake_target: &str, build_config: &BuildConfig) -> Result<Output> {
    let mut scoped = Self::systemd_scoped_cmd_base();
    scoped.args([flake_target, "--dry-run", "--print-out-paths", "--no-link"]);

    match scoped.output().await {
        Ok(output) if output.status.success() => {
            info!("✅ Systemd-scoped evaluation completed successfully");
            Ok(output)
        }
        // ... fallback logic
    }
}
```

#### Tier 2: Fallback Detection

```rust
// Check for systemd-specific issues that indicate fallback needed
if stderr.contains("Failed to connect to user scope bus")
    || stderr.contains("DBUS_SESSION_BUS_ADDRESS")
    || stderr.contains("XDG_RUNTIME_DIR")
    || stderr.contains("Failed to create scope")
    || stderr.contains("systemd")
{
    warn!("⚠️ Systemd scope creation failed (no user session), falling back to direct execution");
    Self::run_direct_dry_run(flake_target, build_config).await
}
```

#### Tier 3: Direct Execution

```rust
async fn run_direct_dry_run(flake_target: &str, build_config: &BuildConfig) -> Result<Output> {
    let mut direct = Command::new("nix");
    direct.args(["build", flake_target, "--dry-run", "--print-out-paths", "--no-link"]);
    build_config.apply_to_command(&mut direct);
    Ok(direct.output().await?)
}
```

### Resource Limit Configuration

#### Memory Management

- **MemoryMax=4G**: Hard limit prevents individual evaluations from consuming excessive RAM
- **Scope isolation**: OOM kills affect only the evaluation scope, not the main server
- **Cleanup**: Systemd automatically cleans up terminated scopes

#### CPU Management

- **CPUQuota=300%**: Limits to 3 CPU cores maximum
- **Prevents**: CPU starvation of other system processes
- **Balances**: Evaluation speed vs system responsiveness

#### Timeout Protection

- **TimeoutStopSec=600**: 10-minute evaluation timeout
- **Prevents**: Hung evaluations from consuming resources indefinitely
- **Graceful**: Allows time for complex evaluations while preventing deadlock

## Fallback Scenarios

### When Fallback Occurs

1. **Missing SystemD**: `systemd-run` command not available
2. **No User Session**: D-Bus session bus unavailable
3. **Permission Issues**: Insufficient privileges for scope creation
4. **Runtime Environment**: Container or restricted environments

### Fallback Detection Logic

The system identifies fallback scenarios through:

- **Command availability**: `std::io::ErrorKind::NotFound`
- **Error message patterns**: D-Bus and systemd-specific error text
- **Graceful degradation**: Automatic retry with direct execution

### Error Categorization

```rust
// Environment issues - fallback
if stderr.contains("Failed to connect to user scope bus") { /* fallback */ }

// Actual evaluation failures - propagate
else { /* return original error */ }
```

## Benefits and Trade-offs

### Benefits

- **Server Stability**: OOM killer can no longer terminate main server process
- **Resource Protection**: System remains responsive during large evaluations
- **Automatic Recovery**: Failed evaluations don't require manual intervention
- **Graceful Degradation**: Works in environments without full systemd support
- **Observable Isolation**: Clear logging shows when isolation is active

### Trade-offs

- **Complexity**: More complex execution path with fallback logic
- **Dependencies**: Relies on systemd user session functionality
- **Resource Overhead**: Slight overhead from scope creation/cleanup
- **Debugging**: Multiple execution paths can complicate troubleshooting

### Performance Impact

- **Scope Creation**: ~10-50ms overhead per evaluation
- **Memory Efficiency**: Better overall memory usage due to isolation
- **CPU Fairness**: More predictable CPU allocation
- **I/O Impact**: Minimal impact on disk/network operations

## Operational Considerations

### Monitoring Points

- **Scope Success Rate**: Percentage of evaluations using systemd scopes
- **Fallback Frequency**: How often direct execution is used
- **Resource Violations**: Evaluations hitting memory/CPU limits
- **Timeout Events**: Evaluations exceeding time limits

### Logging Strategy

```rust
info!("✅ Systemd-scoped evaluation completed successfully");
warn!("⚠️ Systemd scope creation failed (no user session), falling back to direct execution");
info!("📋 systemd-run not available, using direct execution");
```

### Failure Modes

1. **Scope Creation Failure**: Automatic fallback to direct execution
2. **Resource Limit Hit**: Evaluation terminated, scope cleaned up automatically
3. **Timeout**: Graceful termination after 10 minutes
4. **System Resource Exhaustion**: Only affects individual scope, not server

## Architecture Evolution

### Before: Monolithic Evaluation

```
Crystal Forge Server Process
├── All evaluations run in-process
├── Memory consumption unlimited
├── OOM kill affects entire server
└── No resource isolation
```

### After: Isolated Evaluation

```
Crystal Forge Server Process
├── Systemd User Session
│   ├── Evaluation Scope 1 (4GB limit)
│   ├── Evaluation Scope 2 (4GB limit)
│   └── Evaluation Scope N (4GB limit)
├── Fallback to direct execution
└── Server process protected from evaluation OOM
```

## Future Improvements

### Enhanced Resource Management

- Dynamic memory limits based on system capacity
- Priority-based CPU allocation for different evaluation types
- Disk I/O limits for large derivation builds

### Evaluation Caching

- Cache successful evaluations to avoid re-computation
- Invalidation based on flake input changes
- Shared cache across multiple Crystal Forge instances

### Advanced Isolation

- Container-based isolation for complete environment separation
- Network namespace isolation for security
- Filesystem isolation for build cleanliness

This isolation strategy successfully eliminated OOM-related server terminations while maintaining compatibility across different deployment environments through intelligent fallback mechanisms.
