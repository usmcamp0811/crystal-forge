DROP VIEW IF EXISTS view_systems_status_table;

CREATE OR REPLACE VIEW view_systems_status_table AS
WITH current_state AS (
    SELECT DISTINCT ON (hostname)
        hostname,
        derivation_path,
        timestamp AS current_deployed_at
    FROM
        system_states
    ORDER BY
        hostname,
        timestamp DESC
),
current_eval AS (
    SELECT
        et.derivation_path,
        et.target_name,
        et.target_type,
        et.status,
        et.attempt_count,
        et.completed_at,
        et.commit_id
    FROM
        evaluation_targets et
    WHERE
        et.target_type = 'nixos'
),
current_commit AS (
    SELECT
        id,
        git_commit_hash,
        commit_timestamp
    FROM
        commits
),
latest_commit AS (
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
),
joined AS (
    SELECT
        s.hostname,
        vcs.agent_version AS version,
        vcs.uptime_days || 'd' AS uptime,
        vcs.ip_address,
        vcs.last_seen,
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
        systems s
        LEFT JOIN current_state cs ON s.hostname = cs.hostname
        LEFT JOIN current_eval ce ON ce.derivation_path = cs.derivation_path
            AND ce.target_name = s.hostname
        LEFT JOIN current_commit cc ON ce.commit_id = cc.id
        LEFT JOIN latest_commit lc ON s.hostname = lc.hostname
        LEFT JOIN view_systems_current_state vcs ON s.hostname = vcs.hostname
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
        CONCAT(EXTRACT(EPOCH FROM NOW() - last_seen)::int / 60, 'm ago')
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
    last_seen DESC;

