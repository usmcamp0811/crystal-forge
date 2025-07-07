-- Drop existing views if they exist
DROP VIEW IF EXISTS view_systems_current_state, view_current_commit, view_evaluation_pipeline, view_evaluation_queue_status, view_monitored_flake_commits, view_systems_latest_commit, view_systems_latest_flake_commit, view_systems_deployment_status, view_systems_behind, view_systems_inventory, view_systems_attention, view_system_heartbeat_health CASCADE;

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

CREATE VIEW view_systems_current_state AS
SELECT
    vslfc.id AS system_id,
    vslfc.hostname,
    -- Current deployment info
    vslfc.repo_url,
    vslfc.git_commit_hash AS deployed_commit_hash,
    vslfc.commit_timestamp AS deployed_commit_timestamp,
    -- System state info
    lss.derivation_path AS current_derivation_path,
    lss.primary_ip_address AS ip_address,
    ROUND(lss.uptime_secs::numeric / 86400, 1) AS uptime_days,
    lss.os,
    lss.kernel,
    lss.agent_version,
    lss.timestamp AS last_deployed,
    -- Latest commit evaluation info
    latest_eval.derivation_path AS latest_commit_derivation_path,
    latest_eval.evaluation_status AS latest_commit_evaluation_status,
    -- Comparison: is system running the latest evaluated derivation?
    CASE WHEN lss.derivation_path IS NOT NULL
        AND latest_eval.derivation_path IS NOT NULL
        AND lss.derivation_path = latest_eval.derivation_path THEN
        TRUE
    WHEN latest_eval.derivation_path IS NULL THEN
        NULL -- No evaluation exists for latest commit
    ELSE
        FALSE
    END AS is_running_latest_derivation,
    -- Heartbeat info
    lhb.last_heartbeat,
    GREATEST (lss.timestamp, COALESCE(lhb.last_heartbeat, lss.timestamp)) AS last_seen
FROM
    view_systems_latest_flake_commit vslfc
    LEFT JOIN (
        -- Get latest system state per hostname
        SELECT DISTINCT ON (hostname)
            hostname,
            derivation_path,
            primary_ip_address,
            uptime_secs,
            os,
            kernel,
            agent_version,
            timestamp
        FROM
            system_states
        ORDER BY
            hostname,
            timestamp DESC) lss ON vslfc.hostname = lss.hostname
    LEFT JOIN (
        -- Get latest heartbeat per hostname
        SELECT DISTINCT ON (ss.hostname)
            ss.hostname,
            ah.timestamp AS last_heartbeat
        FROM
            system_states ss
            JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
        ORDER BY
            ss.hostname,
            ah.timestamp DESC) lhb ON vslfc.hostname = lhb.hostname
    LEFT JOIN (
        -- Get evaluation for the latest commit for each system
        SELECT
            et.target_name AS hostname,
            et.derivation_path,
            et.status AS evaluation_status
        FROM
            evaluation_targets et
            JOIN view_systems_latest_flake_commit vlc ON et.commit_id = vlc.commit_id
                AND et.target_name = vlc.hostname
        WHERE
            et.status = 'complete') latest_eval ON vslfc.hostname = latest_eval.hostname
ORDER BY
    vslfc.hostname;

COMMENT ON VIEW view_systems_current_state IS 'Complete system overview with latest commit evaluations and derivation comparison';

