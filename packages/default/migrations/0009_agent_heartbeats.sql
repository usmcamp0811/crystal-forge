CREATE TABLE agent_heartbeats (
    id bigserial PRIMARY KEY,
    system_state_id integer NOT NULL REFERENCES system_states (id),
    timestamp timestamptz NOT NULL DEFAULT NOW(),
    agent_version varchar(50),
    agent_build_hash varchar(64)
);

-- Update existing values to the new standardized ones (without restart)
UPDATE
    system_states
SET
    context = CASE WHEN context = 'agent-startup' THEN
        'startup'
    WHEN context = 'agent-loop' THEN
        'config_change'
    WHEN context = 'agent-heartbeat' THEN
        'state_delta'
    ELSE
        context
    END;

-- Rename the column
ALTER TABLE system_states RENAME COLUMN context TO change_reason;

-- Add constraint for the refined values
ALTER TABLE system_states
    ADD CONSTRAINT valid_change_reason CHECK (change_reason IN ('startup', 'config_change', 'state_delta'));

