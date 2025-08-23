DELETE FROM agent_heartbeats
WHERE system_state_id IN (
        SELECT
            id
        FROM
            system_states
        WHERE
            hostname LIKE 'test-deploy-%'
            OR hostname LIKE 'scenario-%');

DELETE FROM derivations
WHERE derivation_name LIKE 'test-deploy-%'
    OR derivation_name LIKE 'scenario-%';

DELETE FROM commits
WHERE git_commit_hash IN ('mapping123', 'old456', 'new789');

DELETE FROM systems
WHERE hostname LIKE 'test-deploy-%'
    OR hostname LIKE 'scenario-%';

DELETE FROM flakes
WHERE name LIKE 'test-%flake';

DELETE FROM system_states
WHERE hostname LIKE 'test-deploy-%'
    OR hostname LIKE 'scenario-%';

DELETE FROM derivation_statuses
WHERE name LIKE 'test-%';

