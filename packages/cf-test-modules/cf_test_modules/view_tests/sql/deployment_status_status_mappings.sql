-- Create test data with known statuses and verify mappings
BEGIN;

-- Create test flake and system
INSERT INTO flakes (name, repo_url) VALUES ('test-deploy-flake', 'http://test-deploy.git');
INSERT INTO systems (hostname, flake_id, derivation, public_key, is_active) 
VALUES ('test-deploy-mapping', (SELECT id FROM flakes WHERE name = 'test-deploy-flake'), 'nixosConfigurations.test', 'test-deploy-key', true);

-- Create commit
INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES 
((SELECT id FROM flakes WHERE name = 'test-deploy-flake'), 'mapping123', NOW() - INTERVAL '1 hour');

-- Create derivation status if not exists
INSERT INTO derivation_statuses (name, description, is_terminal, is_success, display_order) VALUES 
('test-mapping-complete', 'Test Mapping Complete', true, true, 101) ON CONFLICT (name) DO NOTHING;

-- Create derivation
INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id) VALUES 
((SELECT id FROM commits WHERE git_commit_hash = 'mapping123'), 'nixos', 'test-deploy-mapping', '/nix/store/mapping-path', (SELECT id FROM derivation_statuses WHERE name = 'test-mapping-complete'));

-- Create system state to make it "up_to_date"
INSERT INTO system_states (
    hostname, derivation_path, change_reason, os, kernel,
    memory_gb, uptime_secs, cpu_brand, cpu_cores,
    primary_ip_address, nixos_version, agent_compatible,
    timestamp
) VALUES (
    'test-deploy-mapping', '/nix/store/mapping-path', 'startup', '25.05', '6.6.89',
    8192, 3600, 'Test CPU', 4, '10.0.0.400', '25.05', true,
    NOW() - INTERVAL '10 minutes'
);

-- Query to see if our test system shows up with correct mapping
SELECT 
    count,
    status_display
FROM view_deployment_status
WHERE status_display IN ('Up to Date', 'Behind', 'Evaluation Failed', 'No Evaluation', 'No Deployment', 'Never Seen', 'Unknown');

ROLLBACK;
