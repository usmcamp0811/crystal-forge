BEGIN;

-- Test exactly at 1-hour boundary (should be included)
INSERT INTO system_states (
    hostname, derivation_path, change_reason, os, kernel,
    memory_gb, uptime_secs, cpu_brand, cpu_cores,
    primary_ip_address, nixos_version, agent_compatible,
    timestamp
) VALUES 
('test-edge-1hr', '/nix/store/edge-1hr', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.6.1', '25.05', true,
 NOW() - INTERVAL '1 hour'),
-- Test exactly at 4-hour boundary (should be critical, not offline)
('test-edge-4hr', '/nix/store/edge-4hr', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.6.2', '25.05', true,
 NOW() - INTERVAL '4 hours');

-- Add heartbeats
INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
((SELECT id FROM system_states WHERE hostname = 'test-edge-1hr'), NOW() - INTERVAL '1 hour', '1.0.0', 'edge1'),
((SELECT id FROM system_states WHERE hostname = 'test-edge-4hr'), NOW() - INTERVAL '4 hours', '1.0.0', 'edge4');

-- Query edge case systems
SELECT 
    hostname,
    status,
    hours_ago
FROM view_critical_systems
WHERE hostname LIKE 'test-edge-%'
ORDER BY hostname;

ROLLBACK;
