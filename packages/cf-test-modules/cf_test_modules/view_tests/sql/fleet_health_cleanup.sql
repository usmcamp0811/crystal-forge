DELETE FROM agent_heartbeats 
WHERE system_state_id IN (
    SELECT id FROM system_states WHERE hostname LIKE 'test-fleet-%' OR hostname LIKE 'health-scenario-%' OR hostname LIKE 'test-filter-%'
);

DELETE FROM system_states WHERE hostname LIKE 'test-fleet-%' OR hostname LIKE 'health-scenario-%' OR hostname LIKE 'test-filter-%';
