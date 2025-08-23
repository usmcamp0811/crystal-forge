BEGIN;

INSERT INTO system_states (
    hostname, derivation_path, change_reason, os, kernel,
    memory_gb, uptime_secs, cpu_brand, cpu_cores,
    primary_ip_address, nixos_version, agent_compatible,
    timestamp
) VALUES (
    'test-nulls', '/nix/store/test-nulls', 'startup', NULL, NULL,
    NULL, NULL, NULL, NULL, NULL, NULL, true,
    NOW() - INTERVAL '5 minutes'
);

SELECT connectivity_status FROM view_systems_status_table 
WHERE hostname = 'test-nulls';

ROLLBACK;
