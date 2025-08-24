-- Complex deployment states test for view_deployment_status
-- Creates test scenarios that are easier to set up via SQL than through agents
WITH test_scenarios AS (
    -- Insert test flake
    INSERT INTO flakes (name, repo_url, created_at, updated_at)
        VALUES ('test-complex-scenarios', 'https://github.com/test/complex.git', NOW(), NOW())
    ON CONFLICT (repo_url)
        DO UPDATE SET
            name = EXCLUDED.name
        RETURNING
            id AS flake_id
),
test_commits AS (
    -- Insert multiple commits for different scenarios
    INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
    SELECT
        flake_id,
        hash,
        NOW() - (row_number * INTERVAL '1 hour'),
        0
    FROM
        test_scenarios,
        UNNEST(ARRAY['deployed123', 'failed456', 'pending789', 'unknown000'])
        WITH ORDINALITY AS t (hash, row_number)
    ON CONFLICT (git_commit_hash)
        DO UPDATE SET
            commit_timestamp = EXCLUDED.commit_timestamp
        RETURNING
            id AS commit_id,
            git_commit_hash
),
test_derivations AS (
    -- Insert derivations with different statuses
    INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id, attempt_count, scheduled_at, completed_at, started_at, error_message)
    SELECT
        tc.commit_id,
        'nixos',
        CASE tc.git_commit_hash
        WHEN 'deployed123' THEN
            'deployed-system'
        WHEN 'failed456' THEN
            'failed-system'
        WHEN 'pending789' THEN
            'pending-system'
        WHEN 'unknown000' THEN
            'unknown-system'
        END,
        CASE tc.git_commit_hash
        WHEN 'deployed123' THEN
            '/nix/store/deployed1-system.drv'
        WHEN 'failed456' THEN
            '/nix/store/failed456-system.drv'
        WHEN 'pending789' THEN
            NULL -- No path for pending
        WHEN 'unknown000' THEN
            '/nix/store/unknown00-system.drv'
        END,
        CASE tc.git_commit_hash
        WHEN 'deployed123' THEN
            10 -- build-complete
        WHEN 'failed456' THEN
            12 -- build-failed
        WHEN 'pending789' THEN
            3 -- dry-run-pending
        WHEN 'unknown000' THEN
            5 -- dry-run-complete (but no system state)
        END,
        0, -- attempt_count
        NOW() - INTERVAL '2 hours', -- scheduled_at
        CASE tc.git_commit_hash
        WHEN 'deployed123' THEN
            NOW() - INTERVAL '30 minutes'
        WHEN 'failed456' THEN
            NOW() - INTERVAL '45 minutes'
        ELSE
            NULL
        END, -- completed_at
        CASE tc.git_commit_hash
        WHEN 'deployed123' THEN
            NOW() - INTERVAL '1 hour'
        WHEN 'failed456' THEN
            NOW() - INTERVAL '1 hour 15 minutes'
        ELSE
            NULL
        END, -- started_at
        CASE tc.git_commit_hash
        WHEN 'failed456' THEN
            'Build failed: compilation error in module X'
        ELSE
            NULL
        END -- error_message
    FROM
        test_commits tc
    RETURNING
        id AS derivation_id
),
test_system_states AS (
    -- Insert system states for some scenarios
    INSERT INTO system_states (hostname, change_reason, derivation_path, os, kernel, memory_gb, uptime_secs, cpu_brand, cpu_cores, agent_version, nixos_version, agent_compatible, partial_data)
        VALUES ('test-complex-deployed', 'deployment', '/nix/store/deployed1-system.drv', 'NixOS', '6.6.89', 16.0, 3600, 'Test CPU', 8, '1.2.3', '25.05', TRUE, FALSE),
        ('test-complex-failed', 'startup', '/nix/store/old-system.drv', -- Running old version
            'NixOS', '6.6.89', 16.0, 7200, 'Test CPU', 8, '1.2.3', '25.05', TRUE, FALSE),
        ('test-complex-pending', 'heartbeat', '/nix/store/pending-old.drv', -- Old version
            'NixOS', '6.6.89', 16.0, 1800, 'Test CPU', 8, '1.2.3', '25.05', TRUE, FALSE)
        -- Note: no system state for 'unknown' scenario
    RETURNING
        id AS state_id, hostname)
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

