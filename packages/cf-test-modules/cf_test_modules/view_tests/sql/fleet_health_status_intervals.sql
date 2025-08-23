
-- Create test systems with different last_seen timestamps
INSERT INTO system_states (
    hostname, derivation_path, change_reason, os, kernel,
    memory_gb, uptime_secs, cpu_brand, cpu_cores,
    primary_ip_address, nixos_version, agent_compatible,
    timestamp
) VALUES 
-- Healthy: last seen 5 minutes ago
('test-fleet-healthy', '/nix/store/healthy', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.0.100', '25.05', true,
 NOW() - INTERVAL '5 minutes'),
-- Warning: last seen 30 minutes ago  
('test-fleet-warning', '/nix/store/warning', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.0.101', '25.05', true,
 NOW() - INTERVAL '30 minutes'),
-- Critical: last seen 2 hours ago
('test-fleet-critical', '/nix/store/critical', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.0.102', '25.05', true,
 NOW() - INTERVAL '2 hours'),
-- Offline: last seen 8 hours ago
('test-fleet-offline', '/nix/store/offline', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.0.103', '25.05', true,
 NOW() - INTERVAL '8 hours');

-- Add heartbeats with same timestamps to ensure last_seen calculation
INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
((SELECT id FROM system_states WHERE hostname = 'test-fleet-healthy' ORDER BY timestamp DESC LIMIT 1),
 NOW() - INTERVAL '5 minutes', '1.0.0', 'hash1'),
((SELECT id FROM system_states WHERE hostname = 'test-fleet-warning' ORDER BY timestamp DESC LIMIT 1),
 NOW() - INTERVAL '30 minutes', '1.0.0', 'hash2'),
((SELECT id FROM system_states WHERE hostname = 'test-fleet-critical' ORDER BY timestamp DESC LIMIT 1),
 NOW() - INTERVAL '2 hours', '1.0.0', 'hash3'),
((SELECT id FROM system_states WHERE hostname = 'test-fleet-offline' ORDER BY timestamp DESC LIMIT 1),
 NOW() - INTERVAL '8 hours', '1.0.0', 'hash4');

-- Query fleet health status for our test systems
SELECT 
    health_status,
    count
FROM view_fleet_health_status
WHERE health_status IN ('Healthy', 'Warning', 'Critical', 'Offline')
ORDER BY 
    CASE health_status
        WHEN 'Healthy' THEN 1
        WHEN 'Warning' THEN 2
        WHEN 'Critical' THEN 3
        WHEN 'Offline' THEN 4
    END;

