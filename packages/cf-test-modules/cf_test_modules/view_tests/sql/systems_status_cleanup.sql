DELETE FROM agent_heartbeats 
WHERE system_state_id IN (
    SELECT id FROM system_states WHERE hostname LIKE 'test-%'
);

DELETE FROM derivations WHERE derivation_name LIKE 'test-%';

DELETE FROM commits WHERE git_commit_hash LIKE '%test%' OR git_commit_hash IN ('abc123old', 'def456new');

DELETE FROM systems WHERE hostname LIKE 'test-%';

DELETE FROM flakes WHERE name LIKE 'test-%';

DELETE FROM system_states WHERE hostname LIKE 'test-%';
