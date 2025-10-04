# Refactor System Monitoring to Heartbeat + Change Detection Pattern

## Summary

Implement a more efficient storage pattern for system state monitoring by separating heartbeat signals from actual state changes. This will reduce storage growth while maintaining full observability.

## Current State

- Agents report full system state every 10 minutes to a single table
- All fields stored on every report regardless of changes
- Growing storage concerns with high-frequency, mostly-duplicate data

## Proposed Solution

Implement a two-table pattern:

1. **Heartbeat table** - lightweight, high-frequency liveness signals
2. **System state table** - full data, only written on actual changes

## Acceptance Criteria

### Database Schema Changes

- [ ] Create new `agent_heartbeats` table with minimal schema:

  ```sql
  CREATE TABLE agent_heartbeats (
    id BIGSERIAL PRIMARY KEY,
    system_id UUID NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    agent_version VARCHAR(50),
    agent_build_hash VARCHAR(64),
    INDEX idx_heartbeats_system_timestamp (system_id, timestamp)
  );
  ```

- [ ] Modify existing system state table to add change tracking:
  ```sql
  ALTER TABLE system_state ADD COLUMN change_reason VARCHAR(50);
  -- Possible values: 'initial', 'config_change', 'restart', 'state_delta'
  ```

### Application Logic

- [ ] Implement state comparison logic in agent collection service
- [ ] Always insert heartbeat record on agent report
- [ ] Compare incoming state against last known state for system
- [ ] Insert to system_state table only when changes detected
- [ ] Handle special cases (agent restart, config updates) to force state insert

### State Comparison Strategy

- [ ] Decide on comparison method:
  - Option A: Field-by-field comparison
  - Option B: Hash entire payload and compare hashes
- [ ] Implement efficient "last known state" lookup
- [ ] Define which fields should trigger state updates vs. ignored fields

### Data Retention

- [ ] Implement heartbeat cleanup (retain 24-48 hours)
- [ ] Keep system state changes for longer retention period
- [ ] Add database maintenance jobs for cleanup

### Monitoring & Observability

- [ ] Add metrics for:
  - Heartbeat frequency per system
  - State change frequency
  - Storage reduction percentage
- [ ] Alert on missing heartbeats (system down detection)
- [ ] Dashboard showing active systems and last seen times

## Technical Considerations

### Performance

- Proper indexing on heartbeats table for efficient queries
- Consider in-memory caching of last known states
- Batch operations where possible

### Migration Strategy

- [ ] Plan migration from existing single-table approach
- [ ] Backfill logic for existing data
- [ ] Rollback plan if issues arise

### Agent Changes

- [ ] Update agent reporting logic if needed
- [ ] Maintain backward compatibility during transition
- [ ] Update agent documentation

## Testing

- [ ] Unit tests for state comparison logic
- [ ] Integration tests for heartbeat + state change flow
- [ ] Load testing to verify storage efficiency gains
- [ ] Test system down/up detection accuracy

## Documentation

- [ ] Update system monitoring documentation
- [ ] Document new table schemas and relationships
- [ ] Update runbook for troubleshooting system state issues

## Definition of Done

- Agents successfully report heartbeats every 10 minutes
- System state table only receives inserts on actual changes
- Storage growth significantly reduced compared to current approach
- No loss of observability or monitoring capabilities
- All tests passing and documentation updated
