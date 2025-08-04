CREATE OR REPLACE VIEW public.view_systems_status_table AS
WITH current_state AS (
    -- Get the most recent system state for each hostname
    SELECT DISTINCT ON (system_states.hostname)
        system_states.hostname,
        system_states.derivation_path,
        system_states."timestamp" AS current_deployed_at,
        system_states.agent_version,
        ROUND(system_states.uptime_secs / 86400.0, 1) AS uptime_days,
        system_states.primary_ip_address AS ip_address
    FROM
        public.system_states
    ORDER BY
        system_states.hostname,
        system_states."timestamp" DESC
),
current_eval AS (
    -- Get current evaluation status for NixOS derivations
    SELECT
        d.derivation_path,
        d.derivation_name,
        d.derivation_type,
        ds.name AS status,
        d.attempt_count,
        d.completed_at,
        d.commit_id
    FROM
        public.derivations d
        JOIN public.derivation_statuses ds ON d.status_id = ds.id
    WHERE
        d.derivation_type = 'nixos'
),
current_commit AS (
    -- Get commit information
    SELECT
        commits.id,
        commits.git_commit_hash,
        commits.commit_timestamp
    FROM
        public.commits
),
latest_commit AS (
    -- Get the latest commit for each system's flake
    SELECT DISTINCT ON (s.hostname)
        s.hostname,
        c.git_commit_hash AS latest_commit_hash,
        c.commit_timestamp AS latest_commit_timestamp
    FROM
        public.systems s
        JOIN public.flakes f ON s.flake_id = f.id
        JOIN public.commits c ON f.id = c.flake_id
    ORDER BY
        s.hostname,
        c.commit_timestamp DESC
),
joined AS (
    SELECT
        s.hostname,
        cs.agent_version AS version,
        (cs.uptime_days || 'd') AS uptime,
        cs.ip_address,
        cs.current_deployed_at AS last_seen,
        cs.current_deployed_at,
        cs.derivation_path AS current_derivation_path,
        ce.status AS current_eval_status,
        ce.attempt_count AS current_eval_attempts,
        ce.completed_at AS current_eval_completed,
        cc.git_commit_hash AS current_commit_hash,
        cc.commit_timestamp AS current_commit_timestamp,
        lc.latest_commit_hash,
        lc.latest_commit_timestamp
    FROM
        public.systems s
        LEFT JOIN current_state cs ON s.hostname = cs.hostname
        LEFT JOIN current_eval ce ON (ce.derivation_path = cs.derivation_path
                AND ce.derivation_name = s.hostname)
        LEFT JOIN current_commit cc ON ce.commit_id = cc.id
        LEFT JOIN latest_commit lc ON s.hostname = lc.hostname
)
SELECT
    hostname,
    CASE WHEN current_derivation_path IS NULL THEN
        '⚫'
    WHEN current_commit_hash IS NULL THEN
        '🟤'
    WHEN current_commit_hash = latest_commit_hash THEN
        '🟢'
    ELSE
        '🔴'
    END AS status,
    CASE WHEN current_derivation_path IS NULL THEN
        'Offline'
    WHEN current_commit_hash IS NULL THEN
        'Unknown State'
    WHEN current_commit_hash = latest_commit_hash THEN
        'Up to Date'
    ELSE
        'Outdated'
    END AS status_text,
    CASE WHEN last_seen IS NULL THEN
        'never'
    ELSE
        CONCAT((EXTRACT(epoch FROM (NOW() - last_seen))::integer / 60), 'm ago')
    END AS last_seen,
    version,
    uptime,
    ip_address
FROM
    joined
ORDER BY
    CASE WHEN current_commit_hash = latest_commit_hash THEN
        1
    WHEN current_commit_hash IS NULL THEN
        4
    ELSE
        3
    END,
    CASE WHEN last_seen IS NULL THEN
        'never'
    ELSE
        CONCAT((EXTRACT(epoch FROM (NOW() - last_seen))::integer / 60), 'm ago')
    END DESC;

