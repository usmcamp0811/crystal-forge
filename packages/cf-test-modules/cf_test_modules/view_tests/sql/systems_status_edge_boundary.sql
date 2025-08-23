
INSERT INTO system_states (
    hostname, derivation_path, change_reason, os, kernel,
    memory_gb, uptime_secs, cpu_brand, cpu_cores,
    primary_ip_address, nixos_version, agent_compatible,
    timestamp
) VALUES (
    'test-boundary', '/nix/store/test-boundary', 'startup', '25.05', '6.6.89',
    8192, 1800, 'Test CPU', 4, '10.0.0.400', '25.05', true,
    NOW() - INTERVAL '30 minutes'
);

SELECT connectivity_status FROM view_systems_status_table 
WHERE hostname = 'test-boundary';

