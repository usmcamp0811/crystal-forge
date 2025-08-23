DELETE FROM agent_heartbeats 
WHERE system_state_id IN (
    SELECT id FROM system_states 
    WHERE hostname LIKE 'test-critical-%' 
       OR hostname LIKE 'test-offline-%'
       OR hostname LIKE 'test-hours-%'
       OR hostname LIKE 'test-filter-%'
       OR hostname LIKE 'test-sort-%'
       OR hostname LIKE 'test-edge-%'
       OR hostname LIKE 'scenario-%'
);

DELETE FROM system_states 
WHERE hostname LIKE 'test-critical-%' 
   OR hostname LIKE 'test-offline-%'
   OR hostname LIKE 'test-hours-%'
   OR hostname LIKE 'test-filter-%'
   OR hostname LIKE 'test-sort-%'
   OR hostname LIKE 'test-edge-%'
   OR hostname LIKE 'scenario-%';
