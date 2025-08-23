-- Create multiple systems in different critical/offline states
INSERT INTO system_states (hostname, derivation_path, change_reason, os, kernel, memory_gb, uptime_secs, cpu_brand, cpu_cores, primary_ip_address, nixos_version, agent_compatible, timestamp)
    VALUES ('scenario-crit-1', '/nix/store/scenario-crit-1', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.7.1', '25.05', TRUE, NOW() - INTERVAL '1.2 hours'),
    ('scenario-crit-2', '/nix/store/scenario-crit-2', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.7.2', '25.05', TRUE, NOW() - INTERVAL '2.5 hours'),
    ('scenario-crit-3', '/nix/store/scenario-crit-3', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.7.3', '25.05', TRUE, NOW() - INTERVAL '3.8 hours'),
    ('scenario-off-1', '/nix/store/scenario-off-1', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.7.4', '25.05', TRUE, NOW() - INTERVAL '5 hours'),
    ('scenario-off-2', '/nix/store/scenario-off-2', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.7.5', '25.05', TRUE, NOW() - INTERVAL '12 hours');

-- Add heartbeats
INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash)
    VALUES ((
            SELECT
                id
            FROM
                system_states
            WHERE
                hostname = 'scenario-crit-1'), NOW() - INTERVAL '1.2 hours', '1.0.0', 'sc1'), ((
        SELECT
            id
        FROM system_states
        WHERE
            hostname = 'scenario-crit-2'), NOW() - INTERVAL '2.5 hours', '1.0.0', 'sc2'), ((
        SELECT
            id
        FROM system_states
    WHERE
        hostname = 'scenario-crit-3'), NOW() - INTERVAL '3.8 hours', '1.0.0', 'sc3'), ((
        SELECT
            id
        FROM system_states
    WHERE
        hostname = 'scenario-off-1'), NOW() - INTERVAL '5 hours', '1.0.0', 'so1'), ((
        SELECT
            id
        FROM system_states
    WHERE
        hostname = 'scenario-off-2'), NOW() - INTERVAL '12 hours', '1.0.0', 'so2');

-- Query scenarios
SELECT
    status,
    COUNT(*) AS count
FROM
    view_critical_systems
WHERE
    hostname LIKE 'scenario-%'
GROUP BY
    status
ORDER BY
    status;

