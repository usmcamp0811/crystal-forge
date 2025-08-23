
-- Create test flake
INSERT INTO flakes (name, repo_url) VALUES ('test-flake', 'http://test.git');

-- Create test system
INSERT INTO systems (hostname, flake_id, derivation, public_key, is_active) 
VALUES ('test-update-sys', (SELECT id FROM flakes WHERE name = 'test-flake'), 'nixosConfigurations.test', 'test-key', true);

-- Create commits
INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES 
((SELECT id FROM flakes WHERE name = 'test-flake'), 'abc123old', NOW() - INTERVAL '2 hours'),
((SELECT id FROM flakes WHERE name = 'test-flake'), 'def456new', NOW() - INTERVAL '1 hour');

-- Create derivation statuses if not exist
INSERT INTO derivation_statuses (name, description, is_terminal, is_success, display_order) VALUES 
('test-complete', 'Test Complete', true, true, 100) ON CONFLICT (name) DO NOTHING;

-- Create derivations for both commits
INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id) VALUES 
((SELECT id FROM commits WHERE git_commit_hash = 'abc123old'), 'nixos', 'test-update-sys', '/nix/store/old-path', (SELECT id FROM derivation_statuses WHERE name = 'test-complete')),
((SELECT id FROM commits WHERE git_commit_hash = 'def456new'), 'nixos', 'test-update-sys', '/nix/store/new-path', (SELECT id FROM derivation_statuses WHERE name = 'test-complete'));

-- System is running old derivation
INSERT INTO system_states (
    hostname, derivation_path, change_reason, os, kernel,
    memory_gb, uptime_secs, cpu_brand, cpu_cores,
    primary_ip_address, nixos_version, agent_compatible,
    timestamp
) VALUES (
    'test-update-sys', '/nix/store/old-path', 'startup', '25.05', '6.6.89',
    8192, 3600, 'Test CPU', 4, '10.0.0.250', '25.05', true,
    NOW() - INTERVAL '10 minutes'
);

-- Query the view
SELECT 
    hostname,
    connectivity_status,
    update_status,
    update_status_text,
    overall_status,
    current_derivation_path,
    latest_derivation_path,
    drift_hours
FROM view_systems_status_table 
WHERE hostname = 'test-update-sys';

