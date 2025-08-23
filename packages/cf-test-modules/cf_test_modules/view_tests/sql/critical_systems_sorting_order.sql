
-- Create test systems with different hours_ago values
INSERT INTO system_states (
    hostname, derivation_path, change_reason, os, kernel,
    memory_gb, uptime_secs, cpu_brand, cpu_cores,
    primary_ip_address, nixos_version, agent_compatible,
    timestamp
) VALUES 
('test-sort-1hr', '/nix/store/sort-1hr', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.5.1', '25.05', true,
 NOW() - INTERVAL '1.5 hours'),
('test-sort-3hr', '/nix/store/sort-3hr', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.5.2', '25.05', true,
 NOW() - INTERVAL '3 hours'),
('test-sort-6hr', '/nix/store/sort-6hr', 'startup', '25.05', '6.6.89',
 8192, 3600, 'Test CPU', 4, '10.0.5.3', '25.05', true,
 NOW() - INTERVAL '6 hours');

-- Add heartbeats
INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash) VALUES 
((SELECT id FROM system_states WHERE hostname = 'test-sort-1hr'), NOW() - INTERVAL '1.5 hours', '1.0.0', 'sort1'),
((SELECT id FROM system_states WHERE hostname = 'test-sort-3hr'), NOW() - INTERVAL '3 hours', '1.0.0', 'sort3'),
((SELECT id FROM system_states WHERE hostname = 'test-sort-6hr'), NOW() - INTERVAL '6 hours', '1.0.0', 'sort6');

-- Query with expected sort order (hours_ago ascending)
SELECT 
    hostname,
    hours_ago,
    status
FROM view_critical_systems
WHERE hostname LIKE 'test-sort-%'
ORDER BY hours_ago;

