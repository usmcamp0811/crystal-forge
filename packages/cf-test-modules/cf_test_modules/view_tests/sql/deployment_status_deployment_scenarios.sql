
-- Create test flake
INSERT INTO flakes (name, repo_url) VALUES ('test-scenarios-flake', 'http://scenarios.git');

-- Create multiple test systems with different scenarios
INSERT INTO systems (hostname, flake_id, derivation, public_key, is_active) VALUES 
('scenario-uptodate', (SELECT id FROM flakes WHERE name = 'test-scenarios-flake'), 'nixosConfigurations.uptodate', 'key1', true),
('scenario-behind', (SELECT id FROM flakes WHERE name = 'test-scenarios-flake'), 'nixosConfigurations.behind', 'key2', true),
('scenario-never', (SELECT id FROM flakes WHERE name = 'test-scenarios-flake'), 'nixosConfigurations.never', 'key3', true);

-- Create commits
INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES 
((SELECT id FROM flakes WHERE name = 'test-scenarios-flake'), 'old456', NOW() - INTERVAL '2 hours'),
((SELECT id FROM flakes WHERE name = 'test-scenarios-flake'), 'new789', NOW() - INTERVAL '1 hour');

-- Create derivation status
INSERT INTO derivation_statuses (name, description, is_terminal, is_success, display_order) VALUES 
('test-scenarios-complete', 'Scenarios Complete', true, true, 102) ON CONFLICT (name) DO NOTHING;

-- Create derivations
INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id) VALUES 
-- Up to date system - latest derivation
((SELECT id FROM commits WHERE git_commit_hash = 'new789'), 'nixos', 'scenario-uptodate', '/nix/store/uptodate-new', (SELECT id FROM derivation_statuses WHERE name = 'test-scenarios-complete')),
-- Behind system - old derivation  
((SELECT id FROM commits WHERE git_commit_hash = 'old456'), 'nixos', 'scenario-behind', '/nix/store/behind-old', (SELECT id FROM derivation_statuses WHERE name = 'test-scenarios-complete')),
((SELECT id FROM commits WHERE git_commit_hash = 'new789'), 'nixos', 'scenario-behind', '/nix/store/behind-new', (SELECT id FROM derivation_statuses WHERE name = 'test-scenarios-complete'));

-- Create system states
INSERT INTO system_states (hostname, derivation_path, change_reason, timestamp) VALUES 
-- Up to date system running latest
('scenario-uptodate', '/nix/store/uptodate-new', 'startup', NOW() - INTERVAL '10 minutes'),
-- Behind system running old
('scenario-behind', '/nix/store/behind-old', 'startup', NOW() - INTERVAL '10 minutes');
-- scenario-never has no system_states (never seen)

-- Query deployment status
SELECT 
    status_display,
    count
FROM view_deployment_status
WHERE status_display IN ('Up to Date', 'Behind', 'Never Seen')
ORDER BY count DESC;

