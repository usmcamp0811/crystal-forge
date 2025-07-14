-- Crystal Forge Database Seed Data (Safe Version)
-- Run this after your schema migrations are complete
-- Seed flakes (has unique constraint on repo_url)
INSERT INTO flakes (name, repo_url)
SELECT
    'nixos-infrastructure',
    'https://github.com/crystalforge/nixos-infrastructure.git'
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            flakes
        WHERE
            repo_url = 'https://github.com/crystalforge/nixos-infrastructure.git');

INSERT INTO flakes (name, repo_url)
SELECT
    'web-services',
    'https://github.com/crystalforge/web-services-config.git'
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            flakes
        WHERE
            repo_url = 'https://github.com/crystalforge/web-services-config.git');

INSERT INTO flakes (name, repo_url)
SELECT
    'database-cluster',
    'https://github.com/crystalforge/database-cluster-config.git'
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            flakes
        WHERE
            repo_url = 'https://github.com/crystalforge/database-cluster-config.git');

INSERT INTO flakes (name, repo_url)
SELECT
    'monitoring-stack',
    'https://github.com/crystalforge/monitoring-stack.git'
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            flakes
        WHERE
            repo_url = 'https://github.com/crystalforge/monitoring-stack.git');

INSERT INTO flakes (name, repo_url)
SELECT
    'security-baseline',
    'https://github.com/crystalforge/security-baseline.git'
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            flakes
        WHERE
            repo_url = 'https://github.com/crystalforge/security-baseline.git');

-- Seed users (has unique constraint on username and email)
INSERT INTO users (id, username, first_name, last_name, email, user_type, is_active)
SELECT
    gen_random_uuid (),
    'mcamp',
    'Matthew',
    'Camp',
    'matt@crystalforge.dev',
    'human',
    TRUE
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            users
        WHERE
            username = 'mcamp');

INSERT INTO users (id, username, first_name, last_name, email, user_type, is_active)
SELECT
    gen_random_uuid (),
    'ccamp',
    'Candace',
    'Camp',
    'candace@crystalforge.dev',
    'human',
    TRUE
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            users
        WHERE
            username = 'ccamp');

INSERT INTO users (id, username, first_name, last_name, email, user_type, is_active)
SELECT
    gen_random_uuid (),
    'devops1',
    'Alex',
    'Johnson',
    'alex.johnson@crystalforge.dev',
    'human',
    TRUE
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            users
        WHERE
            username = 'devops1');

INSERT INTO users (id, username, first_name, last_name, email, user_type, is_active)
SELECT
    gen_random_uuid (),
    'sysadmin1',
    'Sarah',
    'Chen',
    'sarah.chen@crystalforge.dev',
    'human',
    TRUE
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            users
        WHERE
            username = 'sysadmin1');

INSERT INTO users (id, username, first_name, last_name, email, user_type, is_active)
SELECT
    gen_random_uuid (),
    'monitor-service',
    'Monitor',
    'Service',
    'monitor@crystalforge.dev',
    'service',
    TRUE
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            users
        WHERE
            username = 'monitor-service');

INSERT INTO users (id, username, first_name, last_name, email, user_type, is_active)
SELECT
    gen_random_uuid (),
    'backup-system',
    'Backup',
    'System',
    'backup@crystalforge.dev',
    'system',
    TRUE
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            users
        WHERE
            username = 'backup-system');

-- Seed commits (has unique constraint on flake_id, git_commit_hash)
INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
SELECT
    f.id,
    'a1b2c3d4e5f6789012345678901234567890abcd',
    NOW() - INTERVAL '7 days',
    0
FROM
    flakes f
WHERE
    f.name = 'nixos-infrastructure'
    AND NOT EXISTS (
        SELECT
            1
        FROM
            commits
        WHERE
            git_commit_hash = 'a1b2c3d4e5f6789012345678901234567890abcd');

INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
SELECT
    f.id,
    'b2c3d4e5f6789012345678901234567890abcdef1',
    NOW() - INTERVAL '5 days',
    0
FROM
    flakes f
WHERE
    f.name = 'nixos-infrastructure'
    AND NOT EXISTS (
        SELECT
            1
        FROM
            commits
        WHERE
            git_commit_hash = 'b2c3d4e5f6789012345678901234567890abcdef1');

INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
SELECT
    f.id,
    'c3d4e5f6789012345678901234567890abcdef12',
    NOW() - INTERVAL '2 days',
    0
FROM
    flakes f
WHERE
    f.name = 'nixos-infrastructure'
    AND NOT EXISTS (
        SELECT
            1
        FROM
            commits
        WHERE
            git_commit_hash = 'c3d4e5f6789012345678901234567890abcdef12');

INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
SELECT
    f.id,
    'e5f6789012345678901234567890abcdef123ab4',
    NOW() - INTERVAL '6 days',
    0
FROM
    flakes f
WHERE
    f.name = 'web-services'
    AND NOT EXISTS (
        SELECT
            1
        FROM
            commits
        WHERE
            git_commit_hash = 'e5f6789012345678901234567890abcdef123ab4');

INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
SELECT
    f.id,
    'f6789012345678901234567890abcdef123abc45',
    NOW() - INTERVAL '3 days',
    0
FROM
    flakes f
WHERE
    f.name = 'web-services'
    AND NOT EXISTS (
        SELECT
            1
        FROM
            commits
        WHERE
            git_commit_hash = 'f6789012345678901234567890abcdef123abc45');

INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
SELECT
    f.id,
    '789012345678901234567890abcdef123abcd456',
    NOW() - INTERVAL '1 day',
    0
FROM
    flakes f
WHERE
    f.name = 'web-services'
    AND NOT EXISTS (
        SELECT
            1
        FROM
            commits
        WHERE
            git_commit_hash = '789012345678901234567890abcdef123abcd456');

INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
SELECT
    f.id,
    '89012345678901234567890abcdef123abcde567',
    NOW() - INTERVAL '8 days',
    0
FROM
    flakes f
WHERE
    f.name = 'database-cluster'
    AND NOT EXISTS (
        SELECT
            1
        FROM
            commits
        WHERE
            git_commit_hash = '89012345678901234567890abcdef123abcde567');

INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
SELECT
    f.id,
    '012345678901234567890abcdef123abcdefg789',
    NOW() - INTERVAL '5 days',
    0
FROM
    flakes f
WHERE
    f.name = 'monitoring-stack'
    AND NOT EXISTS (
        SELECT
            1
        FROM
            commits
        WHERE
            git_commit_hash = '012345678901234567890abcdef123abcdefg789');

INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
SELECT
    f.id,
    '2345678901234567890abcdef123abcdefghi9ab',
    NOW() - INTERVAL '9 days',
    0
FROM
    flakes f
WHERE
    f.name = 'security-baseline'
    AND NOT EXISTS (
        SELECT
            1
        FROM
            commits
        WHERE
            git_commit_hash = '2345678901234567890abcdef123abcdefghi9ab');

-- Seed systems (has unique constraint on hostname)
INSERT INTO systems (id, hostname, environment_id, is_active, public_key, flake_id, derivation, created_at, updated_at)
SELECT
    gen_random_uuid (),
    'web01.dev.crystalforge.local',
    (
        SELECT
            id
        FROM
            environments
        WHERE
            name = 'dev'
        LIMIT 1),
TRUE,
'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIG1234567890abcdefghijklmnopqrstuvwxyz1234567890',
(
    SELECT
        id
    FROM
        flakes
    WHERE
        name = 'web-services'), '/nix/store/abc123def456-nixos-system-web01', NOW() - INTERVAL '5 days', NOW() - INTERVAL '1 hour'
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            systems
        WHERE
            hostname = 'web01.dev.crystalforge.local');

INSERT INTO systems (id, hostname, environment_id, is_active, public_key, flake_id, derivation, created_at, updated_at)
SELECT
    gen_random_uuid (),
    'web02.dev.crystalforge.local',
    (
        SELECT
            id
        FROM
            environments
        WHERE
            name = 'dev'
        LIMIT 1),
TRUE,
'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIG2345678901bcdefghijklmnopqrstuvwxyz2345678901',
(
    SELECT
        id
    FROM
        flakes
    WHERE
        name = 'web-services'), '/nix/store/def456ghi789-nixos-system-web02', NOW() - INTERVAL '5 days', NOW() - INTERVAL '2 hours'
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            systems
        WHERE
            hostname = 'web02.dev.crystalforge.local');

INSERT INTO systems (id, hostname, environment_id, is_active, public_key, flake_id, derivation, created_at, updated_at)
SELECT
    gen_random_uuid (),
    'db01.test.crystalforge.local',
    (
        SELECT
            id
        FROM
            environments
        WHERE
            name = 'test'
        LIMIT 1),
TRUE,
'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIG3456789012cdefghijklmnopqrstuvwxyz3456789012',
(
    SELECT
        id
    FROM
        flakes
    WHERE
        name = 'database-cluster'), '/nix/store/ghi789jkl012-nixos-system-db01', NOW() - INTERVAL '3 days', NOW() - INTERVAL '30 minutes'
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            systems
        WHERE
            hostname = 'db01.test.crystalforge.local');

INSERT INTO systems (id, hostname, environment_id, is_active, public_key, flake_id, derivation, created_at, updated_at)
SELECT
    gen_random_uuid (),
    'monitor01.prod.crystalforge.local',
    (
        SELECT
            id
        FROM
            environments
        WHERE
            name = 'prod'
        LIMIT 1),
TRUE,
'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIG4567890123defghijklmnopqrstuvwxyz4567890123',
(
    SELECT
        id
    FROM
        flakes
    WHERE
        name = 'monitoring-stack'), '/nix/store/jkl012mno345-nixos-system-monitor01', NOW() - INTERVAL '7 days', NOW() - INTERVAL '15 minutes'
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            systems
        WHERE
            hostname = 'monitor01.prod.crystalforge.local');

-- Seed evaluation targets (has unique constraint on commit_id, target_type, target_name)
INSERT INTO evaluation_targets (commit_id, target_type, target_name, derivation_path, scheduled_at, completed_at, status, attempt_count)
SELECT
    c.id,
    'nixosConfigurations',
    'web01.dev.crystalforge.local',
    '/nix/store/abc123-web01-config',
    NOW() - INTERVAL '5 days',
    NOW() - INTERVAL '5 days',
    'complete',
    0
FROM
    commits c,
    flakes f
WHERE
    c.git_commit_hash = 'e5f6789012345678901234567890abcdef123ab4'
    AND c.flake_id = f.id
    AND f.name = 'web-services'
    AND NOT EXISTS (
        SELECT
            1
        FROM
            evaluation_targets et
        WHERE
            et.commit_id = c.id
            AND et.target_type = 'nixosConfigurations'
            AND et.target_name = 'web01.dev.crystalforge.local');

INSERT INTO evaluation_targets (commit_id, target_type, target_name, derivation_path, scheduled_at, completed_at, status, attempt_count)
SELECT
    c.id,
    'nixosConfigurations',
    'web02.dev.crystalforge.local',
    '/nix/store/def456-web02-config',
    NOW() - INTERVAL '5 days',
    NOW() - INTERVAL '5 days',
    'complete',
    0
FROM
    commits c,
    flakes f
WHERE
    c.git_commit_hash = 'e5f6789012345678901234567890abcdef123ab4'
    AND c.flake_id = f.id
    AND f.name = 'web-services'
    AND NOT EXISTS (
        SELECT
            1
        FROM
            evaluation_targets et
        WHERE
            et.commit_id = c.id
            AND et.target_type = 'nixosConfigurations'
            AND et.target_name = 'web02.dev.crystalforge.local');

INSERT INTO evaluation_targets (commit_id, target_type, target_name, derivation_path, scheduled_at, completed_at, status, attempt_count)
SELECT
    c.id,
    'nixosConfigurations',
    'monitor01.prod.crystalforge.local',
    '/nix/store/ghi789-monitor01-config',
    NOW() - INTERVAL '2 days',
    NOW() - INTERVAL '2 days',
    'complete',
    0
FROM
    commits c,
    flakes f
WHERE
    c.git_commit_hash = '012345678901234567890abcdef123abcdefg789'
    AND c.flake_id = f.id
    AND f.name = 'monitoring-stack'
    AND NOT EXISTS (
        SELECT
            1
        FROM
            evaluation_targets et
        WHERE
            et.commit_id = c.id
            AND et.target_type = 'nixosConfigurations'
            AND et.target_name = 'monitor01.prod.crystalforge.local');

-- A pending evaluation
INSERT INTO evaluation_targets (commit_id, target_type, target_name, derivation_path, scheduled_at, status, attempt_count)
SELECT
    c.id,
    'nixosConfigurations',
    'web01.dev.crystalforge.local',
    NULL,
    NOW() - INTERVAL '1 day',
    'pending',
    1
FROM
    commits c,
    flakes f
WHERE
    c.git_commit_hash = '789012345678901234567890abcdef123abcd456'
    AND c.flake_id = f.id
    AND f.name = 'web-services'
    AND NOT EXISTS (
        SELECT
            1
        FROM
            evaluation_targets et
        WHERE
            et.commit_id = c.id
            AND et.target_type = 'nixosConfigurations'
            AND et.target_name = 'web01.dev.crystalforge.local');

-- A failed evaluation
INSERT INTO evaluation_targets (commit_id, target_type, target_name, derivation_path, scheduled_at, started_at, completed_at, status, attempt_count)
SELECT
    c.id,
    'nixosConfigurations',
    'db01.test.crystalforge.local',
    NULL,
    NOW() - INTERVAL '3 hours',
    NOW() - INTERVAL '3 hours',
    NOW() - INTERVAL '2 hours',
    'failed',
    2
FROM
    commits c,
    flakes f
WHERE
    c.git_commit_hash = '89012345678901234567890abcdef123abcde567'
    AND c.flake_id = f.id
    AND f.name = 'database-cluster'
    AND NOT EXISTS (
        SELECT
            1
        FROM
            evaluation_targets et
        WHERE
            et.commit_id = c.id
            AND et.target_type = 'nixosConfigurations'
            AND et.target_name = 'db01.test.crystalforge.local');

-- Seed system states (no unique constraints, can insert multiple)
INSERT INTO system_states (hostname, change_reason, derivation_path, os, kernel, memory_gb, uptime_secs, cpu_brand, cpu_cores, primary_mac_address, primary_ip_address, gateway_ip, agent_version, agent_build_hash, nixos_version, agent_compatible, partial_data, timestamp)
    VALUES ('web01.dev.crystalforge.local', 'startup', '/nix/store/abc123def456-nixos-system-web01', 'NixOS', '6.1.63', 8.0, 86400, 'Intel Xeon E5-2680 v4', 4, '52:54:00:12:34:56', '10.0.1.10', '10.0.1.1', '1.2.3', 'abc123def456', '23.05', TRUE, FALSE, NOW() - INTERVAL '2 hours'),
    ('web02.dev.crystalforge.local', 'config_change', '/nix/store/def456ghi789-nixos-system-web02', 'NixOS', '6.1.63', 8.0, 72000, 'Intel Xeon E5-2680 v4', 4, '52:54:00:12:34:57', '10.0.1.11', '10.0.1.1', '1.2.3', 'def456ghi789', '23.05', TRUE, FALSE, NOW() - INTERVAL '3 hours'),
    ('db01.test.crystalforge.local', 'startup', '/nix/store/ghi789jkl012-nixos-system-db01', 'NixOS', '6.1.63', 32.0, 259200, 'Intel Xeon E5-2690 v3', 8, '52:54:00:23:45:67', '10.0.2.20', '10.0.2.1', '1.2.4', 'ghi789jkl012', '23.05', TRUE, FALSE, NOW() - INTERVAL '1 hour'),
    ('monitor01.prod.crystalforge.local', 'state_delta', '/nix/store/jkl012mno345-nixos-system-monitor01', 'NixOS', '6.1.63', 16.0, 604800, 'Intel Xeon E5-2695 v2', 6, '52:54:00:34:56:78', '10.0.3.30', '10.0.3.1', '1.2.4', 'jkl012mno345', '23.05', TRUE, FALSE, NOW() - INTERVAL '20 minutes');

-- Seed agent heartbeats (no unique constraints)
INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash)
SELECT
    ss.id,
    NOW() - INTERVAL '5 minutes',
    '1.2.3',
    'abc123def456'
FROM
    system_states ss
WHERE
    ss.hostname = 'web01.dev.crystalforge.local'
ORDER BY
    ss.timestamp DESC
LIMIT 1;

INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash)
SELECT
    ss.id,
    NOW() - INTERVAL '10 minutes',
    '1.2.3',
    'def456ghi789'
FROM
    system_states ss
WHERE
    ss.hostname = 'web02.dev.crystalforge.local'
ORDER BY
    ss.timestamp DESC
LIMIT 1;

INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash)
SELECT
    ss.id,
    NOW() - INTERVAL '30 minutes',
    '1.2.4',
    'ghi789jkl012'
FROM
    system_states ss
WHERE
    ss.hostname = 'db01.test.crystalforge.local'
ORDER BY
    ss.timestamp DESC
LIMIT 1;

INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash)
SELECT
    ss.id,
    NOW() - INTERVAL '3 minutes',
    '1.2.4',
    'jkl012mno345'
FROM
    system_states ss
WHERE
    ss.hostname = 'monitor01.prod.crystalforge.local'
ORDER BY
    ss.timestamp DESC
LIMIT 1;

-- Output summary
SELECT
    'Database seeded successfully!' AS status,
    (
        SELECT
            COUNT(*)
        FROM
            users) AS users_count,
    (
        SELECT
            COUNT(*)
        FROM
            systems) AS systems_count,
    (
        SELECT
            COUNT(*)
        FROM
            flakes) AS flakes_count,
    (
        SELECT
            COUNT(*)
        FROM
            commits) AS commits_count,
    (
        SELECT
            COUNT(*)
        FROM
            evaluation_targets) AS evaluation_targets_count,
    (
        SELECT
            COUNT(*)
        FROM
            system_states) AS system_states_count,
    (
        SELECT
            COUNT(*)
        FROM
            agent_heartbeats) AS heartbeats_count;

