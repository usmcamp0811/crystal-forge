-- System heartbeat status view
-- Determines system health based on both heartbeats and system state changes
CREATE OR REPLACE VIEW view_system_heartbeat_status AS
WITH latest_system_states AS (
    SELECT DISTINCT ON (hostname)
        hostname,
        timestamp AS last_state_change,
        id AS system_state_id
    FROM
        system_states
    ORDER BY
        hostname,
        timestamp DESC
),
latest_heartbeats AS (
    SELECT DISTINCT ON (ss.hostname)
        ss.hostname,
        ah.timestamp AS last_heartbeat
    FROM
        system_states ss
        JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
    ORDER BY
        ss.hostname,
        ah.timestamp DESC
),
heartbeat_analysis AS (
    SELECT
        COALESCE(lss.hostname, lhb.hostname) AS hostname,
        lss.last_state_change,
        lhb.last_heartbeat,
        GREATEST (COALESCE(lss.last_state_change, '1970-01-01'::timestamptz), COALESCE(lhb.last_heartbeat, '1970-01-01'::timestamptz)) AS most_recent_activity,
    CASE
    -- Online: Either heartbeat OR state change within 30 minutes
    WHEN COALESCE(lhb.last_heartbeat, '1970-01-01'::timestamptz) > (NOW() - INTERVAL '30 minutes')
        OR COALESCE(lss.last_state_change, '1970-01-01'::timestamptz) > (NOW() - INTERVAL '30 minutes') THEN
        'online'
        -- Stale: Most recent activity between 30 minutes and 1 hour ago
    WHEN GREATEST (COALESCE(lss.last_state_change, '1970-01-01'::timestamptz), COALESCE(lhb.last_heartbeat, '1970-01-01'::timestamptz)) > (NOW() - INTERVAL '1 hour') THEN
        'stale'
        -- Offline: No activity for over 1 hour
    ELSE
        'offline'
    END AS heartbeat_status,
    -- Calculate minutes since last activity for detailed info
    EXTRACT(EPOCH FROM (NOW() - GREATEST (COALESCE(lss.last_state_change, '1970-01-01'::timestamptz), COALESCE(lhb.last_heartbeat, '1970-01-01'::timestamptz)))) / 60 AS minutes_since_last_activity
FROM
    latest_system_states lss
    FULL OUTER JOIN latest_heartbeats lhb ON lss.hostname = lhb.hostname
)
SELECT
    hostname,
    heartbeat_status,
    most_recent_activity,
    last_heartbeat,
    last_state_change,
    ROUND(minutes_since_last_activity, 1) AS minutes_since_last_activity,
    CASE heartbeat_status
    WHEN 'online' THEN
        'System is active and responding'
    WHEN 'stale' THEN
        'System may be experiencing issues - no recent activity'
    WHEN 'offline' THEN
        'System appears to be offline or unreachable'
    END AS status_description
FROM
    heartbeat_analysis
ORDER BY
    CASE heartbeat_status
    WHEN 'offline' THEN
        1
    WHEN 'stale' THEN
        2
    WHEN 'online' THEN
        3
    END,
    minutes_since_last_activity DESC;

-- System deployment status view
-- Shows where each system stands relative to the latest commit in their flake
CREATE OR REPLACE VIEW view_system_deployment_status AS
WITH latest_system_states AS (
    SELECT DISTINCT ON (hostname)
        hostname,
        derivation_path,
        timestamp AS deployment_time
    FROM
        system_states
    ORDER BY
        hostname,
        timestamp DESC
),
system_current_derivations AS (
    SELECT
        lss.hostname,
        lss.derivation_path,
        lss.deployment_time,
        d.id AS derivation_id,
        d.commit_id AS current_commit_id,
        c.git_commit_hash AS current_commit_hash,
        c.commit_timestamp AS current_commit_timestamp,
        c.flake_id,
        f.name AS flake_name
    FROM
        latest_system_states lss
        LEFT JOIN derivations d ON lss.derivation_path = d.derivation_path
        LEFT JOIN commits c ON d.commit_id = c.id
        LEFT JOIN flakes f ON c.flake_id = f.id
),
latest_flake_commits AS (
    SELECT DISTINCT ON (s.hostname)
        s.hostname,
        c.id AS latest_commit_id,
        c.git_commit_hash AS latest_commit_hash,
        c.commit_timestamp AS latest_commit_timestamp,
        c.flake_id
    FROM
        systems s
        JOIN flakes f ON s.flake_id = f.id
        JOIN commits c ON f.id = c.flake_id
    ORDER BY
        s.hostname,
        c.commit_timestamp DESC
),
commit_counts AS (
    SELECT
        scd.hostname,
        COUNT(newer_commits.id) AS commits_behind
FROM
    system_current_derivations scd
    JOIN latest_flake_commits lfc ON scd.hostname = lfc.hostname
        LEFT JOIN commits newer_commits ON (newer_commits.flake_id = lfc.flake_id
                AND newer_commits.commit_timestamp > scd.current_commit_timestamp
                AND newer_commits.commit_timestamp <= lfc.latest_commit_timestamp)
    WHERE
        scd.current_commit_id IS NOT NULL
        AND lfc.latest_commit_id IS NOT NULL
    GROUP BY
        scd.hostname
)
SELECT
    COALESCE(s.hostname, scd.hostname, lfc.hostname) AS hostname,
    scd.derivation_path AS current_derivation_path,
    scd.deployment_time,
    scd.current_commit_hash,
    scd.current_commit_timestamp,
    lfc.latest_commit_hash,
    lfc.latest_commit_timestamp,
    COALESCE(cc.commits_behind, 0) AS commits_behind,
    scd.flake_name,
    CASE
    -- No deployment: system exists but no system_state
    WHEN scd.hostname IS NULL THEN
        'no_deployment'
        -- Unknown: has deployment but can't relate to any flake/commit
    WHEN scd.flake_id IS NULL THEN
        'unknown'
        -- Up to date: running the latest commit
    WHEN scd.current_commit_id = lfc.latest_commit_id THEN
        'up_to_date'
        -- Behind: running an older commit
    WHEN scd.current_commit_id != lfc.latest_commit_id
        AND scd.current_commit_timestamp < lfc.latest_commit_timestamp THEN
        'behind'
        -- Edge case: somehow running a newer commit than latest (shouldn't happen)
    WHEN scd.current_commit_timestamp > lfc.latest_commit_timestamp THEN
        'ahead'
    ELSE
        'unknown'
    END AS deployment_status,
    CASE WHEN scd.hostname IS NULL THEN
        'System registered but never deployed'
    WHEN scd.flake_id IS NULL THEN
        'Cannot determine flake relationship'
    WHEN scd.current_commit_id = lfc.latest_commit_id THEN
        'Running latest commit'
    WHEN scd.current_commit_id != lfc.latest_commit_id
        AND scd.current_commit_timestamp < lfc.latest_commit_timestamp THEN
        CONCAT('Behind by ', COALESCE(cc.commits_behind, 0), ' commit(s)')
    WHEN scd.current_commit_timestamp > lfc.latest_commit_timestamp THEN
        'Running newer commit than expected'
    ELSE
        'Deployment status unclear'
    END AS status_description
FROM
    systems s
    FULL OUTER JOIN system_current_derivations scd ON s.hostname = scd.hostname
    LEFT JOIN latest_flake_commits lfc ON COALESCE(s.hostname, scd.hostname) = lfc.hostname
    LEFT JOIN commit_counts cc ON COALESCE(s.hostname, scd.hostname) = cc.hostname
WHERE
    s.is_active = TRUE
    OR s.is_active IS NULL
ORDER BY
    CASE
    -- No deployment: system exists but no system_state
    WHEN scd.hostname IS NULL THEN
        1
        -- Unknown: has deployment but can't relate to any flake/commit
    WHEN scd.flake_id IS NULL THEN
        2
        -- Behind: running an older commit
    WHEN scd.current_commit_id != lfc.latest_commit_id
        AND scd.current_commit_timestamp < lfc.latest_commit_timestamp THEN
        3
        -- Up to date: running the latest commit
    WHEN scd.current_commit_id = lfc.latest_commit_id THEN
        4
        -- Edge case: somehow running a newer commit than latest
    WHEN scd.current_commit_timestamp > lfc.latest_commit_timestamp THEN
        5
    ELSE
        6
    END,
    commits_behind DESC NULLS LAST,
    hostname;

