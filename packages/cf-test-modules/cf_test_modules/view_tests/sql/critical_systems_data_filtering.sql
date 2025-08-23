-- Create test systems with different conditions
INSERT INTO system_states (hostname, derivation_path, change_reason, os, kernel, memory_gb, uptime_secs, cpu_brand, cpu_cores, primary_ip_address, nixos_version, agent_compatible, timestamp)
    VALUES
    -- Should be included: 2 hours ago (critical)
    ('test-filter-include', '/nix/store/filter-include', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.4.1', '25.05', TRUE, NOW() - INTERVAL '2 hours'),
    -- Should be excluded: 30 minutes ago (too recent)
    ('test-filter-exclude', '/nix/store/filter-exclude', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.4.2', '25.05', TRUE, NOW() - INTERVAL '30 minutes'),
    -- Should be included: 8 hours ago (offline)
    ('test-filter-offline', '/nix/store/filter-offline', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.4.3', '25.05', TRUE, NOW() - INTERVAL '8 hours');

-- Add heartbeats
INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash)
    VALUES ((
            SELECT
                id
            FROM
                system_states
            WHERE
                hostname = 'test-filter-include'), NOW() - INTERVAL '2 hours', '1.0.0', 'include'), ((
        SELECT
            id
        FROM system_states
        WHERE
            hostname = 'test-filter-exclude'), NOW() - INTERVAL '30 minutes', '1.0.0', 'exclude'), ((
        SELECT
            id
        FROM system_states
    WHERE
        hostname = 'test-filter-offline'), NOW() - INTERVAL '8 hours', '1.0.0', 'offline');

-- Query filtered results
SELECT
    hostname,
    status,
    hours_ago
FROM
    view_critical_systems
WHERE
    hostname LIKE 'test-filter-%'
ORDER BY
    hostname;

