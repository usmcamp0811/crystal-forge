BEGIN;
INSERT INTO system_states (hostname, derivation_path, change_reason, os, kernel, memory_gb, uptime_secs, cpu_brand, cpu_cores, primary_ip_address, nixos_version, agent_compatible, timestamp)
    VALUES ('test-starting', '/nix/store/test-starting', 'startup', '25.05', '6.6.89', 16384, 3600, 'Test CPU', 8, '10.0.0.200', '25.05', TRUE, NOW() - INTERVAL '10 minutes');
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
FROM
    view_systems_status_table
WHERE
    hostname = 'test-starting';
ROLLBACK;

