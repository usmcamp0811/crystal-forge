BEGIN;

-- Create multiple test systems in different health states
INSERT INTO system_states (
    hostname, derivation_path, change_reason, os, kernel,
    memory_gb, uptime_secs, cpu_brand, cpu_cores,
    primary_ip_address, nixos_version, agent_compatible,
    timestamp
) VALUES 
('health-scenario-1', '/nix/store/scenario1', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.1.1', '25.05', true, NOW() - INTERVAL '5 minutes'),
('health-scenario-2', '/nix/store/scenario2', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.1.2', '25.05', true, NOW() - INTERVAL '5 minutes'),
('health-scenario-3', '/nix/store/scenario3', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.1.3', '25.05', true, NOW() - INTERVAL '45 minutes'),
('health-scenario-4', '/nix/store/scenario4', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.1.4', '25.05', true, NOW() - INTERVAL '3 hours'),
('health-scenario-5', '/nix/store/scenario5', 'startup', '25.05', '6.6.89', 8192, 3600, 'Test CPU', 4, '10.0.1.5', '25.05', true, NOW() - INTERVAL '6 hours');

-- Add heartbeats to create different health statuses
INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
((SELECT id FROM system_states WHERE hostname = 'health-scenario-1'), NOW() - INTERVAL '5 minutes', '1.0.0', 'healthy1'),
((SELECT id FROM system_states WHERE hostname = 'health-scenario-2'), NOW() - INTERVAL '5 minutes', '1.0.0', 'healthy2'),
((SELECT id FROM system_states WHERE hostname = 'health-scenario-3'), NOW() - INTERVAL '45 minutes', '1.0.0', 'warning1'),
((SELECT id FROM system_states WHERE hostname = 'health-scenario-4'), NOW() - INTERVAL '3 hours', '1.0.0', 'critical1'),
((SELECT id FROM system_states WHERE hostname = 'health-scenario-5'), NOW() - INTERVAL '6 hours', '1.0.0', 'offline1');

-- Query fleet health status
SELECT 
    health_status,
    count
FROM view_fleet_health_status
WHERE health_status IN ('Healthy', 'Warning', 'Critical', 'Offline')
ORDER BY count DESC;

ROLLBACK;
