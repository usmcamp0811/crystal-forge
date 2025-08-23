
INSERT INTO system_states (
    hostname, derivation_path, change_reason, os, kernel,
    memory_gb, uptime_secs, cpu_brand, cpu_cores,
    primary_ip_address, nixos_version, agent_compatible,
    timestamp
) VALUES (
    'test-stale', '/nix/store/test-stale', 'startup', '25.05', '6.6.89',
    8192, 86400, 'Test CPU', 4, '10.0.0.204', '25.05', true,
    NOW() - INTERVAL '2 hours'
);

INSERT INTO agent_heartbeats (
    system_state_id, timestamp, agent_version, agent_build_hash
) VALUES (
    (SELECT id FROM system_states WHERE hostname = 'test-stale' ORDER BY timestamp DESC LIMIT 1),
    NOW() - INTERVAL '1 hour',
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
WHERE hostname = 'test-stale';

