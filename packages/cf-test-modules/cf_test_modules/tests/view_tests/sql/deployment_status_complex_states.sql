-- Complex deployment states test for view_deployment_status
-- Creates test scenarios that are easier to set up via SQL than through agents
-- Fixed version that works with PostgreSQL CTE limitations
DO $$
DECLARE
    test_flake_id integer;
    deployed_commit_id integer;
    failed_commit_id integer;
    pending_commit_id integer;
    unknown_commit_id integer;
BEGIN
    -- Step 1: Insert test flake
    INSERT INTO flakes (name, repo_url, created_at, updated_at)
        VALUES ('test-complex-scenarios', 'https://github.com/test/complex.git', NOW(), NOW())
    ON CONFLICT (repo_url)
        DO UPDATE SET
            name = EXCLUDED.name
        RETURNING
            id INTO test_flake_id;
    -- Step 2: Insert test commits
    INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
        VALUES (test_flake_id, 'deployed123', NOW() - INTERVAL '1 hour', 0),
        (test_flake_id, 'failed456', NOW() - INTERVAL '2 hours', 0),
        (test_flake_id, 'pending789', NOW() - INTERVAL '3 hours', 0),
        (test_flake_id, 'unknown000', NOW() - INTERVAL '4 hours', 0)
    ON CONFLICT (git_commit_hash)
        DO UPDATE SET
            commit_timestamp = EXCLUDED.commit_timestamp;
    -- Get commit IDs
    SELECT
        id INTO deployed_commit_id
    FROM
        commits
    WHERE
        git_commit_hash = 'deployed123';
    SELECT
        id INTO failed_commit_id
    FROM
        commits
    WHERE
        git_commit_hash = 'failed456';
    SELECT
        id INTO pending_commit_id
    FROM
        commits
    WHERE
        git_commit_hash = 'pending789';
    SELECT
        id INTO unknown_commit_id
    FROM
        commits
    WHERE
        git_commit_hash = 'unknown000';
    -- Step 3: Insert derivations with different statuses
    INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id, attempt_count, scheduled_at, completed_at, started_at, error_message)
        VALUES
        -- Deployed scenario: build complete (status_id = 10)
        (deployed_commit_id, 'nixos', 'deployed-system', '/nix/store/deployed1-system.drv', 10, 1, NOW() - INTERVAL '1 hour', NOW() - INTERVAL '30 minutes', NOW() - INTERVAL '45 minutes', NULL),
        -- Failed scenario: build failed (status_id = 12)
        (failed_commit_id, 'nixos', 'failed-system', '/nix/store/failed456-system.drv', 12, 1, NOW() - INTERVAL '2 hours', NOW() - INTERVAL '90 minutes', NOW() - INTERVAL '105 minutes', 'Build failed: compilation error in module X'),
        -- Pending scenario: build pending (status_id = 7)
        (pending_commit_id, 'nixos', 'pending-system', NULL, 7, 0, NOW() - INTERVAL '3 hours', NULL, NULL, NULL),
        -- Unknown scenario: dry-run complete but no system state (status_id = 5)
        (unknown_commit_id, 'nixos', 'unknown-system', '/nix/store/unknown000-system.drv', 5, 1, NOW() - INTERVAL '4 hours', NOW() - INTERVAL '210 minutes', NOW() - INTERVAL '225 minutes', NULL)
    ON CONFLICT (commit_id, derivation_name)
        DO UPDATE SET
            status_id = EXCLUDED.status_id,
            derivation_path = EXCLUDED.derivation_path,
            error_message = EXCLUDED.error_message;
    -- Step 4: Insert system states for some scenarios
    INSERT INTO system_states (hostname, change_reason, derivation_path, os, kernel, memory_gb, uptime_secs, cpu_brand, cpu_cores, agent_version, nixos_version, agent_compatible, partial_data, timestamp)
        VALUES ('test-complex-deployed', 'deployment', '/nix/store/deployed1-system.drv', 'NixOS', '6.6.89', 16.0, 3600, 'Test CPU', 8, '1.2.3', '25.05', TRUE, FALSE, NOW()),
        ('test-complex-failed', 'startup', '/nix/store/old-system.drv', -- Running old version
            'NixOS', '6.6.89', 16.0, 7200, 'Test CPU', 8, '1.2.3', '25.05', TRUE, FALSE, NOW()),
        ('test-complex-pending', 'heartbeat', '/nix/store/pending-old.drv', -- Old version
            'NixOS', '6.6.89', 16.0, 1800, 'Test CPU', 8, '1.2.3', '25.05', TRUE, FALSE, NOW())
        -- Note: no system state for 'unknown' scenario
    ON CONFLICT (hostname)
        DO UPDATE SET
            derivation_path = EXCLUDED.derivation_path,
            timestamp = EXCLUDED.timestamp;
END
$$;

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

