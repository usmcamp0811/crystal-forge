
-- Create test system with known timestamp
INSERT INTO system_states (
    hostname, derivation_path, change_reason, os, kernel,
    memory_gb, uptime_secs, cpu_brand, cpu_cores,
    primary_ip_address, nixos_version, agent_compatible,
    timestamp
) VALUES 
('test-hours-calc', '/nix/store/hours-calc', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.3.1', '25.05', true,
 NOW() - INTERVAL '2.5 hours');

-- Add heartbeat
INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
((SELECT id FROM system_states WHERE hostname = 'test-hours-calc'), NOW() - INTERVAL '2.5 hours', '1.0.0', 'hourscalc');

-- Query hours_ago calculation
SELECT 
    hostname,
    hours_ago,
    status
FROM view_critical_systems
WHERE hostname = 'test-hours-calc';

