BEGIN;

-- Create test systems with specific timestamps
INSERT INTO system_states (
    hostname, derivation_path, change_reason, os, kernel,
    memory_gb, uptime_secs, cpu_brand, cpu_cores,
    primary_ip_address, nixos_version, agent_compatible,
    timestamp
) VALUES 
-- Critical: last seen 2 hours ago (between 1-4 hours)
('test-critical-2hr', '/nix/store/critical-2hr', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.2.1', '25.05', true,
 NOW() - INTERVAL '2 hours'),
-- Critical: last seen 3 hours ago (between 1-4 hours)
('test-critical-3hr', '/nix/store/critical-3hr', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.2.2', '25.05', true,
 NOW() - INTERVAL '3 hours'),
-- Offline: last seen 6 hours ago (more than 4 hours)
('test-offline-6hr', '/nix/store/offline-6hr', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.2.3', '25.05', true,
 NOW() - INTERVAL '6 hours');

-- Add heartbeats with matching timestamps
INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
((SELECT id FROM system_states WHERE hostname = 'test-critical-2hr'), NOW() - INTERVAL '2 hours', '1.2.3', 'critical2hr'),
((SELECT id FROM system_states WHERE hostname = 'test-critical-3hr'), NOW() - INTERVAL '3 hours', '1.2.4', 'critical3hr'),
((SELECT id FROM system_states WHERE hostname = 'test-offline-6hr'), NOW() - INTERVAL '6 hours', '1.2.5', 'offline6hr');

-- Query our test systems from the critical systems view
SELECT 
    hostname,
    status,
    hours_ago,
    ip,
    version
FROM view_critical_systems
WHERE hostname LIKE 'test-critical-%' OR hostname LIKE 'test-offline-%'
ORDER BY hostname;

ROLLBACK;
