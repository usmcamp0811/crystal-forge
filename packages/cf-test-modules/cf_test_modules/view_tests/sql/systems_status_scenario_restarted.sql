BEGIN;

INSERT INTO system_states (
    hostname, derivation_path, change_reason, os, kernel,
    memory_gb, uptime_secs, cpu_brand, cpu_cores,
    primary_ip_address, nixos_version, agent_compatible,
    timestamp
) VALUES (
    'test-restarted', '/nix/store/test-restarted', 'startup', '25.05', '6.6.89',
    16384, 1800, 'Test CPU', 8, '10.0.0.203', '25.05', true,
    NOW() - INTERVAL '5 minutes'
);

INSERT INTO agent_heartbeats (
    system_state_id, timestamp, agent_version, agent_build_hash
) VALUES (
    (SELECT id FROM system_states WHERE hostname = 'test-restarted' ORDER BY timestamp DESC LIMIT 1),
    NOW() - INTERVAL '2 hours',
    '1.2.3',
    'abc123def'
);

SELECT 
    hostname,
    connectivity_status,
    connectivity_status_text,
    update_status,
    overall_status,
    last_seen,
    agent_version,
    uptime,
    ip_address
FROM view_systems_status_table 
WHERE hostname = 'test-restarted';

ROLLBACK;
