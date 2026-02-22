# Title

<!--
Short, outcome-focused title
-->

---

# Problem

<!--
Brief description of the issue or opportunity.
Keep this lightweight.
-->

---

# Desired Outcome

<!--
What should be true if this is completed?
-->

---

# Notes

<!--
Optional context, links, screenshots, or references.
-->

---

# Scope Hint (Optional)

<!--
If obvious, describe rough boundaries.
Not required at Backlog stage.
-->\n\n# Issue Details\n\n- **Issue ID:** 174611753\n- **Issue IID:** 103\n- **Title:** System-Aware Parallel Build Queue\n- **State:** closed\n- **Labels:** Labels:\n- **Created by:** Matt\n- **Created at:** 2025-10-07T02:08:16.638Z\n- **Updated at:** 2025-10-11T03:03:23.633Z\n\n# Description\n\n## Problem

Current build workers operate independently without coordination:
- No guarantee that all package dependencies are built before attempting NixOS system builds
- Workers can idle while waiting for slow packages even when other work is available
- No concept of "system-level parallelism" - can't assign multiple workers to one system's packages

**Example**: With 5 workers and System A having 3 remaining packages:
- Current: 1 worker builds a package, 4 workers idle or work on unrelated systems
- Desired: 3 workers build System A's packages in parallel, 2 workers help with next system or work down the queue

## Goals

1. **Newest commits first** - Always prioritize most recent commits
2. **Dependencies before systems** - Build all packages for a NixOS system before building the system itself
3. **Parallel derivation building** - Multiple workers can build different packages for the same system simultaneously
4. **Work from top to bottom** - Workers claim work sequentially from a sorted queue, no artificial worker limits per system
5. **Observable queue state** - Use views for easy debugging and Grafana visualization

## Proposed Solution

### Overview

Replace single-worker claiming with a reservation-based system that:
- Tracks which worker owns which derivation
- Groups derivations by their parent NixOS system
- Blocks system builds until all package dependencies are complete
- Allows workers to claim work from a globally sorted queue top-to-bottom
- Provides views for monitoring and debugging

### Database Changes

#### New Table: `build_reservations`

Tracks worker ownership and enables coordination.

**Purpose**: 
- Prevent double-claiming of derivations
- Track which worker is building what
- Group work by NixOS system for coordination
- Detect dead/crashed workers via heartbeat

**Fields**:
- `worker_id` (TEXT): Unique identifier for worker instance (UUID or hostname)
- `derivation_id` (INTEGER): What this worker is building right now
- `nixos_derivation_id` (INTEGER): Which NixOS system this package belongs to (NULL for system builds)
- `reserved_at` (TIMESTAMPTZ): When work was claimed
- `heartbeat_at` (TIMESTAMPTZ): Last heartbeat timestamp
- Unique constraint on `derivation_id` prevents double-claiming

**Indexes**:
- `worker_id` - for worker-specific queries
- `nixos_derivation_id` - for system-level coordination queries
- `heartbeat_at` - for cleanup queries

#### New View: `view_buildable_derivations`

Replaces `view_nixos_derivation_build_queue` with system-aware logic.

**Purpose**: 
- Show all derivations that are ready to be claimed
- Exclude already-reserved derivations
- Sort by commit timestamp DESC (newest first)
- Optionally sort systems by package count (smallest first) to avoid starvation
- Only show NixOS systems where ALL packages are complete
- Enable Grafana dashboards to visualize queue state

**Columns to include**:
- `id` - derivation ID
- `derivation_name` - human-readable name
- `derivation_type` - 'package' or 'nixos'
- `pname`, `version` - package metadata
- `nixos_id` - which NixOS system this belongs to
- `nixos_commit_ts` - commit timestamp for sorting
- `build_type` - 'package' or 'system'
- `total_packages` - total packages for this system
- `completed_packages` - how many packages are done
- `active_workers` - how many workers currently on this system
- `queue_position` - sequential position in queue (for monitoring)

**Sorting logic**:
1. Commit timestamp DESC (newest first)
2. Within same commit, optionally sort by package count ASC (smallest systems first)
3. Packages before systems (packages = 0, systems = 1)
4. Package name

**Filtering logic**:
- Only include derivations with status 'DryRunComplete' or 'Scheduled'
- Exclude derivations already in `build_reservations`
- Only include NixOS systems where `total_packages = completed_packages`
- Exclude derivations with `attempt_count > 5`

#### Additional View: `view_build_queue_status`

**Purpose**: Monitoring and debugging view for Grafana dashboards

**Shows**:
- Each NixOS system's progress (packages complete vs total)
- Which workers are building which derivations
- How long each reservation has been active
- Systems blocked waiting for dependencies
- Idle workers vs active workers
- Queue depth by commit

**Use cases**:
- Debug why a system isn't building (dependencies incomplete)
- Identify stuck builds (old heartbeats)
- Monitor worker utilization
- Visualize queue progression over time

### Application Logic Changes

#### Worker Claiming Strategy

Workers claim work by:
1. Checking if they already have an active reservation (resume interrupted work)
2. If not, query `view_buildable_derivations LIMIT 1` for the next available work
3. Atomically create a reservation and mark derivation as in-progress
4. If claim fails (race condition), loop and try again

**Key principle**: Workers always work from the top of the sorted queue. No intelligent routing or system selection - just take the next item.

#### Worker Heartbeat

Each worker updates its reservation heartbeat every 30 seconds while building. This enables detection of crashed/hung workers.

#### Stale Reservation Cleanup

Background task runs every 60 seconds to:
- Find reservations where `heartbeat_at` is older than 5 minutes
- Delete those reservations
- Reset those derivations back to 'Scheduled' status
- Log which derivations were reclaimed

#### Worker Lifecycle Changes

Workers now:
1. Generate a unique `worker_id` on startup
2. Spawn a heartbeat task that runs for the worker's lifetime
3. Claim work using the new reservation-based system
4. After completing/failing a build, delete the reservation
5. Loop back to claim more work

#### Build Completion/Failure

When a build completes or fails:
1. Start a transaction
2. Delete the reservation from `build_reservations`
3. Update the derivation status (complete or failed)
4. Commit transaction

This ensures reservations are always cleaned up, even on failures.

## Implementation Checklist

### Database
- [x] Create `build_reservations` table with indexes
- [x] Create `view_buildable_derivations` view
- [x] Create `view_build_queue_status` view for monitoring
- [x] Add migration script
- [x] Test views with sample data

### Core Functions
- [x] Implement reservation-based claiming logic
- [x] Implement atomic claim with FOR UPDATE SKIP LOCKED
- [x] Implement check for existing worker reservations
- [x] Implement reservation cleanup on build completion
- [x] Implement reservation cleanup on build failure

### Worker Management
- [x] Update `build_worker()` to generate unique worker_id
- [x] Update `build_worker()` to use reservation claiming
- [x] Implement worker heartbeat loop
- [x] Implement stale reservation cleanup background task
- [x] Update `run_build_loop()` to spawn cleanup task
- [x] Ensure reservations are cleaned up on worker shutdown

### Cleanup
- [x] Remove old `claim_next_derivation()` function
- [x] Remove `view_nixos_derivation_build_queue` view
- [x] Update any code referencing old view

### Testing
- [x] Test multiple workers claiming from same queue
- [x] Test worker crash/recovery scenarios
- [x] Test dead worker cleanup
- [x] Test priority ordering (newest commits first)
- [x] Test blocking of system builds until all packages complete
- [x] Load test with 10 workers and 50 systems

### Monitoring
- [x] Create Grafana dashboard using `view_build_queue_status`
- [ ] Add metrics for worker utilization
- [ ] Add metrics for queue depth by commit
- [ ] Add alerts for stale reservations
- [ ] Add alerts for systems blocked on dependencies for >1 hour

## Metrics to Monitor

- **Worker Utilization**: % of time workers are building vs idle
- **System Build Time**: Time from first package start to system completion
- **Queue Depth**: Number of derivations waiting to start
- **Active Workers by System**: Distribution of workers across systems
- **Stale Reservations**: Frequency of cleanup reclaiming work
- **Queue Position Over Time**: Track how fast derivations move through queue
- **Blocked Systems**: Systems waiting for dependencies

## Design Decisions

### Question: Should we limit max workers per system?
**Decision**: No. Workers claim from the top of a globally sorted queue. If a system has many packages at the top of the queue, many workers will naturally work on it. When those packages complete, workers move to the next items in the queue.

### Question: How to handle systems with 100+ packages?
**Decision**: Optionally sort systems by package count ASC (smallest first) within the same commit timestamp. This gives quick wins for small systems while still respecting newest-first ordering. Can be toggled via config or A/B tested.

### Question: Embedded query vs view?
**Decision**: Use a view (`view_buildable_derivations`) so the queue logic is:
- Visible in the database for debugging
- Easily queryable from Grafana
- Testable independently of Rust code
- Reusable across multiple queries if needed

### Question: When to build NixOS systems?
**Decision**: Systems only appear in `view_buildable_derivations` when ALL their package dependencies are complete. This is enforced in the view's WHERE clause, making it impossible for workers to claim incomplete systems.

## Future Enhancements

- Build time estimation using historical data for better queue visualization
- Distributed tracing for tracking work items across workers
- Dynamic worker scaling based on queue depth
- Build result caching to skip already-built derivations across commits
- Per-commit parallelism limits to prevent one huge commit from starving others\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n