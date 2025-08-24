-- Complex deployment states test for view_deployment_status
-- Creates test scenarios using simpler SQL that works with all PostgreSQL versions
-- Clean up any existing test data first
DELETE FROM system_states
WHERE hostname LIKE 'test-complex-%';

DELETE FROM derivations
WHERE commit_id IN (
        SELECT
            c.id
        FROM
            commits c
            JOIN flakes f ON c.flake_id = f.id
        WHERE
            f.name = 'test-complex-scenarios');

DELETE FROM commits
WHERE flake_id IN (
        SELECT
            id
        FROM
            flakes
        WHERE
            name = 'test-complex-scenarios');

DELETE FROM flakes
WHERE name = 'test-complex-scenarios';

-- Step 1: Insert test flake
INSERT INTO flakes (name, repo_url, created_at, updated_at)
    VALUES ('test-complex-scenarios', 'https://github.com/test/complex.git', NOW(), NOW());

-- Step 2: Insert test commits
INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
SELECT
    f.id,
    commit_data.hash,
    commit_data.timestamp,
    0
FROM
    flakes f,
    LATERAL (
        VALUES ('deployed123', NOW() - INTERVAL '1 hour'),
            ('failed456', NOW() - INTERVAL '2 hours'),
            ('pending789', NOW() - INTERVAL '3 hours'),
            ('unknown000', NOW() - INTERVAL '4 hours')) AS commit_data (hash, timestamp)
WHERE
    f.name = 'test-complex-scenarios';

-- Step 3: Insert derivations with different statuses
INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id, attempt_count, scheduled_at, completed_at, started_at, error_message)
SELECT
    c.id,
    'nixos',
    CASE c.git_commit_hash
    WHEN 'deployed123' THEN
        'deployed-system'
    WHEN 'failed456' THEN
        'failed-system'
    WHEN 'pending789' THEN
        'pending-system'
    WHEN 'unknown000' THEN
        'unknown-system'
    END,
    CASE c.git_commit_hash
    WHEN 'deployed123' THEN
        '/nix/store/deployed1-system.drv'
    WHEN 'failed456' THEN
        '/nix/store/failed456-system.drv'
    WHEN 'pending789' THEN
        NULL
    WHEN 'unknown000' THEN
        '/nix/store/unknown000-system.drv'
    END,
    CASE c.git_commit_hash
    WHEN 'deployed123' THEN
        10 -- build-complete
    WHEN 'failed456' THEN
        12 -- build-failed
    WHEN 'pending789' THEN
        7 -- build-pending
    WHEN 'unknown000' THEN
        5 -- dry-run-complete
    END,
    CASE c.git_commit_hash
    WHEN 'pending789' THEN
        0
    ELSE
        1
    END,
    c.commit_timestamp,
    CASE c.git_commit_hash
    WHEN 'pending789' THEN
        NULL
    ELSE
        c.commit_timestamp + INTERVAL '15 minutes'
    END,
    CASE c.git_commit_hash
    WHEN 'pending789' THEN
        NULL
    ELSE
        c.commit_timestamp + INTERVAL '5 minutes'
    END,
    CASE c.git_commit_hash
    WHEN 'failed456' THEN
        'Build failed: compilation error in module X'
    ELSE
        NULL
    END
FROM
    commits c
    JOIN flakes f ON c.flake_id = f.id
WHERE
    f.name = 'test-complex-scenarios';

-- Step 4: Insert system states for some scenarios
INSERT INTO system_states (hostname, change_reason, derivation_path, os, kernel, memory_gb, uptime_secs, cpu_brand, cpu_cores, agent_version, nixos_version, agent_compatible, partial_data, timestamp)
    VALUES ('test-complex-deployed', 'deployment', '/nix/store/deployed1-system.drv', 'NixOS', '6.6.89', 16.0, 3600, 'Test CPU', 8, '1.2.3', '25.05', TRUE, FALSE, NOW()),
    ('test-complex-failed', 'startup', '/nix/store/old-system.drv', 'NixOS', '6.6.89', 16.0, 7200, 'Test CPU', 8, '1.2.3', '25.05', TRUE, FALSE, NOW()),
    ('test-complex-pending', 'heartbeat', '/nix/store/pending-old.drv', 'NixOS', '6.6.89', 16.0, 1800, 'Test CPU', 8, '1.2.3', '25.05', TRUE, FALSE, NOW());

-- Note: no system state for 'unknown' scenario to test that case
-- Query the view to see results
SELECT
    hostname,
    flake_name,
    deployment_status,
    deployment_status_text,
    build_status,
    commit_hash,
    error_message
FROM
    view_deployment_status
WHERE
    flake_name = 'test-complex-scenarios'
ORDER BY
    CASE deployment_status
    WHEN 'deployed' THEN
        1
    WHEN 'build-failed' THEN
        2
    WHEN 'pending' THEN
        3
    WHEN 'unknown' THEN
        4
    ELSE
        5
    END;

