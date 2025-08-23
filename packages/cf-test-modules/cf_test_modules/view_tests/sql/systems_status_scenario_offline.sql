INSERT INTO system_states (hostname, derivation_path, change_reason, os, kernel, memory_gb, uptime_secs, cpu_brand, cpu_cores, primary_ip_address, nixos_version, agent_compatible, timestamp)
    VALUES ('test-offline', '/nix/store/test-offline', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.0.201', '25.05', TRUE, NOW() - INTERVAL '2 hours');
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
    hostname = 'test-offline';
