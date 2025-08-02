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

CREATE OR REPLACE VIEW view_current_dry_runs AS
SELECT
    et.id,
    et.target_name AS hostname,
    et.derivation_path,
    et.status,
    et.attempt_count,
    et.scheduled_at,
    et.completed_at,
    c.git_commit_hash,
    c.commit_timestamp
FROM
    evaluation_targets et
    JOIN commits c ON et.commit_id = c.id
WHERE
    et.target_type = 'nixos'
    AND et.status IN ('dry-run-pending', 'dry-run-inprogress')
ORDER BY
    et.scheduled_at DESC;

CREATE OR REPLACE VIEW view_current_builds AS
SELECT
    et.id,
    et.target_name AS hostname,
    et.derivation_path,
    et.status,
    et.attempt_count,
    et.scheduled_at,
    et.completed_at,
    c.git_commit_hash,
    c.commit_timestamp
FROM
    evaluation_targets et
    JOIN commits c ON et.commit_id = c.id
WHERE
    et.target_type = 'nixos'
    AND et.status IN ('build-pending', 'build-inprogress', 'pending', 'queued', 'in-progress')
ORDER BY
    et.scheduled_at DESC;

CREATE OR REPLACE VIEW view_current_scans AS
SELECT
    et.id,
    et.target_name AS hostname,
    et.derivation_path,
    et.status,
    et.attempt_count,
    et.scheduled_at,
    et.completed_at,
    c.git_commit_hash,
    c.commit_timestamp
FROM
    evaluation_targets et
    JOIN commits c ON et.commit_id = c.id
WHERE
    et.target_type = 'nixos'
    AND et.status IN ('scan-pending', 'scan-inprogress')
ORDER BY
    et.scheduled_at DESC;

CREATE OR REPLACE VIEW view_current_commits AS
WITH latest_commits AS (
    SELECT DISTINCT ON (flake_id)
        c.id,
        c.flake_id,
        f.name AS flake_name,
        c.git_commit_hash,
        c.commit_timestamp,
        c.attempt_count
    FROM
        commits c
        JOIN flakes f ON c.flake_id = f.id
    ORDER BY
        c.flake_id,
        c.commit_timestamp DESC
),
eval_progress AS (
    SELECT
        et.commit_id,
        COUNT(*) AS total_systems,
    -- Progress counts
    COUNT(*) FILTER (WHERE et.status IN ('dry-run-complete', 'dry-run-failed', 'build-pending', 'build-inprogress', 'build-complete', 'build-failed')) AS completed_dry_run,
    COUNT(*) FILTER (WHERE et.status = 'build-complete') AS build_completed,
    -- Raw status counts (no assumptions)
    COUNT(*) FILTER (WHERE et.status = 'dry-run-pending') AS dry_run_pending,
    COUNT(*) FILTER (WHERE et.status = 'dry-run-inprogress') AS dry_run_inprogress,
    COUNT(*) FILTER (WHERE et.status = 'dry-run-complete') AS dry_run_complete,
    COUNT(*) FILTER (WHERE et.status = 'dry-run-failed') AS dry_run_failed,
    COUNT(*) FILTER (WHERE et.status = 'build-pending') AS build_pending,
    COUNT(*) FILTER (WHERE et.status = 'build-inprogress') AS build_inprogress,
    COUNT(*) FILTER (WHERE et.status = 'build-complete') AS build_complete,
    COUNT(*) FILTER (WHERE et.status = 'build-failed') AS build_failed
FROM
    evaluation_targets et
GROUP BY
    et.commit_id
)
SELECT
    lc.id,
    lc.flake_name,
    lc.git_commit_hash,
    LEFT (lc.git_commit_hash,
        8) AS short_hash,
    lc.commit_timestamp,
    lc.attempt_count,
    -- Progress summaries
    CASE WHEN ep.total_systems IS NULL THEN
        'N/A'
    ELSE
        CONCAT(ep.completed_dry_run, '/', ep.total_systems)
    END AS dry_run_completed,
    CASE WHEN ep.total_systems IS NULL THEN
        'N/A'
    ELSE
        CONCAT(ep.build_completed, '/', ep.total_systems)
    END AS build_completed,
    -- Full breakdown
    ep.total_systems,
    ep.dry_run_pending,
    ep.dry_run_inprogress,
    ep.dry_run_complete,
    ep.dry_run_failed,
    ep.build_pending,
    ep.build_inprogress,
    ep.build_complete,
    ep.build_failed
FROM
    latest_commits lc
    LEFT JOIN eval_progress ep ON lc.id = ep.commit_id
ORDER BY
    lc.commit_timestamp DESC;

CREATE OR REPLACE VIEW view_failed_evaluations AS
SELECT
    et.id,
    et.target_name AS hostname,
    et.derivation_path,
    et.status,
    et.attempt_count,
    et.scheduled_at,
    et.completed_at,
    c.git_commit_hash,
    c.commit_timestamp,
    f.name AS flake_name
FROM
    evaluation_targets et
    JOIN commits c ON et.commit_id = c.id
    JOIN flakes f ON c.flake_id = f.id
WHERE
    et.status IN ('dry-run-failed', 'build-failed')
ORDER BY
    et.completed_at DESC NULLS LAST;

CREATE OR REPLACE VIEW view_failed_cve_scan_status AS
SELECT
    cs.id AS scan_id,
    et.id AS evaluation_target_id,
    et.target_name AS evaluation_target_name,
    f.name AS flake_name,
    c.git_commit_hash,
    et.derivation_path,
    cs.status,
    cs.attempts,
    cs.scanner_name,
    cs.scanner_version,
    cs.scheduled_at,
    cs.completed_at
FROM
    cve_scans cs
    JOIN evaluation_targets et ON cs.evaluation_target_id = et.id
    JOIN commits c ON et.commit_id = c.id
    JOIN flakes f ON c.flake_id = f.id
WHERE
    cs.status = 'failed'
ORDER BY
    cs.completed_at DESC NULLS LAST;

CREATE OR REPLACE VIEW view_failed_build_commits AS
SELECT
    c.id AS commit_id,
    f.name AS flake_name,
    c.git_commit_hash,
    c.commit_timestamp,
    COUNT(et.id) FILTER (WHERE et.status = 'build-failed') AS failed_targets,
    COUNT(et.id) AS total_targets,
    ARRAY_AGG(DISTINCT et.target_name ORDER BY et.target_name) FILTER (WHERE et.status = 'build-failed') AS failed_hostnames
FROM
    commits c
    JOIN flakes f ON c.flake_id = f.id
    JOIN evaluation_targets et ON et.commit_id = c.id
WHERE
    et.status = 'build-failed'
GROUP BY
    c.id,
    f.name,
    c.git_commit_hash,
    c.commit_timestamp
ORDER BY
    c.commit_timestamp DESC;

CREATE OR REPLACE VIEW view_failed_dry_run_commits AS
SELECT
    c.id AS commit_id,
    f.name AS flake_name,
    c.git_commit_hash,
    c.commit_timestamp,
    COUNT(et.id) FILTER (WHERE et.status = 'dry-run-failed') AS failed_targets,
    COUNT(et.id) AS total_targets,
    ARRAY_AGG(DISTINCT et.target_name ORDER BY et.target_name) FILTER (WHERE et.status = 'dry-run-failed') AS failed_hostnames
FROM
    commits c
    JOIN flakes f ON c.flake_id = f.id
    JOIN evaluation_targets et ON et.commit_id = c.id
WHERE
    et.status = 'dry-run-failed'
GROUP BY
    c.id,
    f.name,
    c.git_commit_hash,
    c.commit_timestamp
ORDER BY
    c.commit_timestamp DESC;

CREATE OR REPLACE VIEW view_commits_stuck_in_evaluation AS
SELECT
    c.id AS commit_id,
    f.name AS flake_name,
    c.git_commit_hash,
    c.commit_timestamp,
    c.attempt_count
FROM
    commits c
    JOIN flakes f ON c.flake_id = f.id
WHERE
    c.attempt_count >= 5
ORDER BY
    c.commit_timestamp DESC;

CREATE OR REPLACE VIEW view_commit_deployment_timeline AS
SELECT
    c.flake_id,
    f.name AS flake_name,
    c.id AS commit_id,
    c.git_commit_hash,
    LEFT (c.git_commit_hash,
        8) AS short_hash,
    c.commit_timestamp,
    -- Show evaluation info
    COUNT(DISTINCT et.id) AS total_evaluations,
    COUNT(DISTINCT et.id) FILTER (WHERE et.derivation_path IS NOT NULL) AS successful_evaluations,
    STRING_AGG(DISTINCT et.status, ', ') AS evaluation_statuses,
    STRING_AGG(DISTINCT et.target_name, ', ') AS evaluated_targets,
    -- Deployment info (only for successful evaluations with derivation_path)
    MIN(ss.timestamp) AS first_deployment,
    MAX(ss.timestamp) AS last_deployment,
    COUNT(DISTINCT ss.hostname) AS total_systems_deployed,
    COUNT(DISTINCT current_sys.hostname) AS currently_deployed_systems,
    STRING_AGG(DISTINCT ss.hostname, ', ') AS deployed_systems,
    STRING_AGG(DISTINCT CASE WHEN current_sys.hostname IS NOT NULL THEN
            current_sys.hostname
        END, ', ') AS currently_deployed_systems_list
FROM
    commits c
    JOIN flakes f ON c.flake_id = f.id
    LEFT JOIN evaluation_targets et ON c.id = et.commit_id
        AND et.target_type = 'nixos'
        AND et.status IN ('build-failed', 'build-complete', 'dry-run-complete', 'build-pending', 'build-inprogress') -- Only successful evaluations
    LEFT JOIN system_states ss ON et.derivation_path = ss.derivation_path
        AND et.derivation_path IS NOT NULL -- Only join for successful evaluations
    LEFT JOIN ( SELECT DISTINCT ON (hostname)
            hostname,
            derivation_path
        FROM
            system_states
        ORDER BY
            hostname,
            timestamp DESC) current_sys ON et.derivation_path = current_sys.derivation_path
    AND et.target_name = current_sys.hostname
    AND et.derivation_path IS NOT NULL -- Only for successful evaluations
WHERE
    c.commit_timestamp >= NOW() - INTERVAL '30 days'
GROUP BY
    c.flake_id,
    c.id,
    c.git_commit_hash,
    c.commit_timestamp,
    f.name
ORDER BY
    c.commit_timestamp DESC;

COMMENT ON VIEW view_commit_deployment_timeline IS 'Shows commit deployment timeline for successfully evaluated commits that have been deployed to systems.';

