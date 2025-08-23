
-- Create test systems with edge case last_seen values
INSERT INTO system_states (
    hostname, derivation_path, change_reason, os, kernel,
    memory_gb, uptime_secs, cpu_brand, cpu_cores,
    primary_ip_address, nixos_version, agent_compatible,
    timestamp
) VALUES 
-- System with NULL primary_ip_address but valid timestamp
('test-filter-null-ip', '/nix/store/null-ip', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, NULL, '25.05', true,
 NOW() - INTERVAL '10 minutes'),
-- System that should be filtered out (no last_seen data)
('test-filter-no-data', '/nix/store/no-data', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.0.200', '25.05', true,
 NOW() - INTERVAL '10 minutes');

-- Add heartbeat only for the first system
INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
((SELECT id FROM system_states WHERE hostname = 'test-filter-null-ip' ORDER BY timestamp DESC LIMIT 1),
 NOW() - INTERVAL '10 minutes', '1.0.0', 'filtertest');

-- The second system will have no heartbeats, so last_seen might be just the system_state timestamp

-- Check what our test systems look like in the base view
SELECT 
    hostname,
    last_seen,
    ip_address
FROM view_systems_status_table 
WHERE hostname LIKE 'test-filter-%'
ORDER BY hostname;

