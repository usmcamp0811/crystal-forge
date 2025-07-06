-- View 1: Systems Current State
-- Shows each system with their current running configuration
-- Drop existing views if they exist
DROP VIEW IF EXISTS view_current_commit, view_evaluation_pipeline, view_evaluation_queue_status, view_monitored_flake_commits, view_systems_current_state, view_systems_latest_commit, view_systems_latest_flake_commit CASCADE;

-- Recreate view: view_systems_current_state
CREATE VIEW view_systems_current_state AS
SELECT
    s.id,
    s.hostname,
    ss.primary_ip_address AS ip_address,
    ss.derivation_path AS current_derivation_path,
    ROUND(ss.uptime_secs::numeric / 86400, 1) AS uptime_days,
    ss.os,
    ss.kernel,
    ss.agent_version,
    ss.id AS ssid,
    ss.timestamp AS last_state_change,
    ah.timestamp AS last_heartbeat
FROM
    systems s
    LEFT JOIN LATERAL (
        SELECT
            *
        FROM
            system_states ss
        WHERE
            ss.hostname = s.hostname
        ORDER BY
            ss.timestamp DESC
        LIMIT 1) ss ON TRUE
    LEFT JOIN LATERAL (
        SELECT
            ah.timestamp
        FROM
            agent_heartbeats ah
        WHERE
            ah.system_state_id = ss.id
        ORDER BY
            ah.timestamp DESC
        LIMIT 1) ah ON TRUE;

COMMENT ON VIEW view_systems_current_state IS 'Shows each system with current config, last deployment time, and true last seen (including heartbeats).';

-- Recreate view: view_monitored_flake_commits
CREATE VIEW view_monitored_flake_commits AS
SELECT
    c.id AS commit_id,
    f.name,
    f.repo_url,
    c.git_commit_hash,
    c.commit_timestamp
FROM
    flakes f
    JOIN commits c ON f.id = c.flake_id
ORDER BY
    c.commit_timestamp DESC;

COMMENT ON VIEW view_monitored_flake_commits IS 'Shows all commits for monitored flakes, including flake name, repo URL, commit hash, and timestamp, ordered by most recent first.';

-- Recreate view: view_current_commit
CREATE VIEW view_current_commit AS
SELECT
    c.id AS commit_id,
    f.name,
    f.repo_url,
    c.git_commit_hash,
    c.commit_timestamp
FROM (
    SELECT
        *,
        ROW_NUMBER() OVER (PARTITION BY flake_id ORDER BY commit_timestamp DESC) AS rn
    FROM
        commits) c
    JOIN flakes f ON f.id = c.flake_id
WHERE
    c.rn = 1;

COMMENT ON VIEW view_current_commit IS 'Shows the most recent commit for each monitored flake, including commit and flake information.';

CREATE VIEW view_systems_latest_flake_commit AS
SELECT
    latest.system_id AS id,
    latest.hostname,
    latest.commit_id,
    latest.git_commit_hash,
    latest.commit_timestamp,
    latest.repo_url
FROM (
    SELECT
        et.*,
        s.id AS system_id,
        s.hostname,
        f.id AS flake_id,
        c.commit_timestamp,
        c.git_commit_hash,
        f.repo_url,
        ROW_NUMBER() OVER (PARTITION BY s.id, f.id ORDER BY c.commit_timestamp DESC) AS rn
    FROM
        evaluation_targets et
        JOIN systems s ON et.target_name = s.hostname
        JOIN commits c ON c.id = et.commit_id
        JOIN flakes f ON f.id = c.flake_id) latest
WHERE
    latest.rn = 1;

COMMENT ON VIEW view_systems_latest_flake_commit IS 'Shows the latest commit for each flake evaluated by each system, including commit ID, timestamp, and repository URL.';

CREATE VIEW view_systems_deployment_status AS
SELECT
    sys.hostname,
    sys.repo_url,
    sys.git_commit_hash AS deployed_commit,
    curr.git_commit_hash AS latest_commit,
    (sys.git_commit_hash IS NOT NULL
        AND curr.git_commit_hash IS NOT NULL
        AND sys.git_commit_hash = curr.git_commit_hash) AS is_up_to_date
FROM
    view_systems_latest_flake_commit sys
    LEFT JOIN view_current_commit curr ON sys.repo_url = curr.repo_url
ORDER BY
    sys.hostname;

COMMENT ON VIEW view_systems_deployment_status IS 'Shows each system’s currently deployed flake commit, the latest commit for that flake, and whether the system is up to date.';

--- Stuff i made
