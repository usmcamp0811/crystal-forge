CREATE OR REPLACE VIEW public.view_systems_status_table AS
WITH current_state AS (
    -- Get the most recent system state for each hostname
    SELECT DISTINCT ON (hostname)
        hostname,
        derivation_path,
        "timestamp" AS current_deployed_at,
        agent_version,
        ROUND(uptime_secs / 86400.0, 1) AS uptime_days,
        primary_ip_address AS ip_address
    FROM
        system_states
    ORDER BY
        hostname,
        "timestamp" DESC
),
latest_heartbeat AS (
    -- Get the most recent heartbeat (actual "last seen") for each hostname
    SELECT DISTINCT ON (ss.hostname)
        ss.hostname,
        ah.timestamp AS last_heartbeat
    FROM
        system_states ss
        JOIN agent_heartbeats ah ON ss.id = ah.system_state_id
    ORDER BY
        ss.hostname,
        ah.timestamp DESC
),
latest_successful_derivation AS (
    -- For each system, get the most recent successful derivation
    SELECT DISTINCT ON (d.derivation_name)
        d.derivation_name,
        d.commit_id,
        d.derivation_path AS derivation_drv_path,
        ds.name AS status,
        c.git_commit_hash,
        c.commit_timestamp
    FROM
        derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        LEFT JOIN commits c ON d.commit_id = c.id
    WHERE
        d.derivation_type = 'nixos'
        AND ds.name IN ('dry-run-complete', 'build-pending', 'build-in-progress', 'build-complete', 'build-failed', 'complete')
    ORDER BY
        d.derivation_name,
        c.commit_timestamp DESC
),
latest_commit AS (
    -- Get the latest commit for each system's flake
    SELECT DISTINCT ON (s.hostname)
        s.hostname,
        c.git_commit_hash AS latest_commit_hash,
        c.commit_timestamp AS latest_commit_timestamp
    FROM
        systems s
        JOIN flakes f ON s.flake_id = f.id
        JOIN commits c ON f.id = c.flake_id
    ORDER BY
        s.hostname,
        c.commit_timestamp DESC
)
SELECT
    s.hostname,
    CASE WHEN cs.derivation_path IS NULL THEN
        '⚫'
    WHEN lsd.git_commit_hash IS NULL THEN
        '🟤'
    WHEN lsd.git_commit_hash = lc.latest_commit_hash THEN
        '🟢'
    ELSE
        '🔴'
    END AS status,
    CASE WHEN cs.derivation_path IS NULL THEN
        'Offline'
    WHEN lsd.git_commit_hash IS NULL THEN
        'Unknown State'
    WHEN lsd.git_commit_hash = lc.latest_commit_hash THEN
        'Up to Date'
    ELSE
        'Outdated'
    END AS status_text,
    CASE WHEN lhb.last_heartbeat IS NULL THEN
        'never'
    ELSE
        CONCAT((EXTRACT(epoch FROM (NOW() - lhb.last_heartbeat))::integer / 60), 'm ago')
    END AS last_seen,
    cs.agent_version AS version,
    (cs.uptime_days || 'd') AS uptime,
    cs.ip_address
FROM
    systems s
    LEFT JOIN current_state cs ON s.hostname = cs.hostname
    LEFT JOIN latest_heartbeat lhb ON s.hostname = lhb.hostname
    LEFT JOIN latest_successful_derivation lsd ON s.hostname = lsd.derivation_name
    LEFT JOIN latest_commit lc ON s.hostname = lc.hostname
ORDER BY
    CASE WHEN lsd.git_commit_hash = lc.latest_commit_hash THEN
        1 -- Up to date first
    WHEN lsd.git_commit_hash IS NULL THEN
        4 -- Unknown state last
    WHEN cs.derivation_path IS NULL THEN
        5 -- Offline last
    ELSE
        3 -- Outdated in middle
    END,
    CASE WHEN lhb.last_heartbeat IS NULL THEN
        'never'
    ELSE
        CONCAT((EXTRACT(epoch FROM (NOW() - lhb.last_heartbeat))::integer / 60), 'm ago')
    END DESC;

