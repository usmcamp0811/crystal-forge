BEGIN;

-- Create a system with multiple states and heartbeats
INSERT INTO system_states (
    hostname, derivation_path, change_reason, os, kernel,
    memory_gb, uptime_secs, cpu_brand, cpu_cores,
    primary_ip_address, nixos_version, agent_compatible,
    timestamp
) VALUES 
-- Older state
('test-multi', '/nix/store/test-multi-old', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.0.300', '25.05', true,
 NOW() - INTERVAL '2 hours'),
-- Newer state 
('test-multi', '/nix/store/test-multi-new', 'config_change', '25.05', '6.6.89',
 16384, 7200, 'Test CPU', 8, '10.0.0.301', '25.05', true,
 NOW() - INTERVAL '10 minutes');

-- Add heartbeats for both states
INSERT INTO agent_heartbeats (
    system_state_id, timestamp, agent_version, agent_build_hash
) VALUES 
-- Old heartbeat on old state
((SELECT id FROM system_states WHERE hostname = 'test-multi' ORDER BY timestamp ASC LIMIT 1),
 NOW() - INTERVAL '1.5 hours', '1.0.0', 'old123'),
-- Recent heartbeat on new state  
((SELECT id FROM system_states WHERE hostname = 'test-multi' ORDER BY timestamp DESC LIMIT 1),
 NOW() - INTERVAL '5 minutes', '1.2.0', 'new456');

-- Query the view and validate it uses the latest system state and heartbeat
SELECT 
    hostname,
    connectivity_status,
    connectivity_status_text,
    agent_version,
    ip_address,
    uptime
FROM view_systems_status_table 
WHERE hostname = 'test-multi';

ROLLBACK;
