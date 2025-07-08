CREATE OR REPLACE VIEW view_systems_summary AS
SELECT
    (
        SELECT
            COUNT(*)
        FROM
            view_systems_current_state) AS "Total Systems",
    (
        SELECT
            COUNT(*)
        FROM
            view_systems_current_state
        WHERE
            is_running_latest_derivation = TRUE) AS "Up to Date",
    (
        SELECT
            COUNT(*)
        FROM
            view_systems_current_state
        WHERE
            is_running_latest_derivation = FALSE) AS "Behind Latest",
    (
        SELECT
            COUNT(*)
        FROM
            view_systems_current_state
        WHERE
            last_seen < NOW() - INTERVAL '15 minutes') AS "No Recent Heartbeat";

COMMENT ON VIEW view_systems_summary IS 'Summary view providing key system metrics:
- "Total Systems": total systems reporting
- "Up to Date": systems running latest derivation
- "Behind Latest": systems not on latest derivation
- "No Recent Heartbeat": systems without heartbeat in last 15 minutes.';

CREATE OR REPLACE VIEW view_systems_status_table AS
SELECT
    hostname,
    -- Determine status symbol
    CASE WHEN current_derivation_path IS NULL THEN
        '⚫' -- No System State
    WHEN latest_commit_derivation_path IS NULL THEN
        '⚪' -- No Evaluation
    WHEN current_derivation_path = latest_commit_derivation_path THEN
        '🟢' -- Current
    WHEN latest_commit_evaluation_status != 'complete' THEN
        '🟡' -- Eval Pending
    ELSE
        '🔴' -- Behind
    END AS status,
    CONCAT(EXTRACT(EPOCH FROM NOW() - last_seen)::int / 60, 'm ago') AS last_seen,
    agent_version AS version,
    uptime_days || 'd' AS uptime,
    ip_address
FROM
    view_systems_current_state
ORDER BY
    CASE WHEN current_derivation_path = latest_commit_derivation_path THEN
        1 -- 🟢
    WHEN latest_commit_evaluation_status != 'complete' THEN
        2 -- 🟡
    ELSE
        3 -- 🔴/others
    END,
    last_seen;

COMMENT ON VIEW view_systems_status_table IS 'Provides system status with hostname, status symbol, last seen time, agent version, uptime, and IP for Grafana dashboards.';

CREATE OR REPLACE VIEW view_systems_drift_time AS
SELECT
    hostname,
    current_derivation_path,
    latest_commit_derivation_path,
    deployed_commit_timestamp,
    -- Calculate how long system has been drifted
    CASE WHEN is_running_latest_derivation = FALSE
        AND deployed_commit_timestamp IS NOT NULL THEN
        EXTRACT(EPOCH FROM (NOW() - deployed_commit_timestamp)) / 3600 -- hours
    ELSE
        0
    END AS drift_hours,
    CASE WHEN is_running_latest_derivation = FALSE
        AND deployed_commit_timestamp IS NOT NULL THEN
        EXTRACT(EPOCH FROM (NOW() - deployed_commit_timestamp)) / 86400 -- days
    ELSE
        0
    END AS drift_days,
    is_running_latest_derivation,
    last_seen
FROM
    view_systems_current_state
WHERE
    is_running_latest_derivation = FALSE
ORDER BY
    drift_hours DESC;

COMMENT ON VIEW view_systems_drift_time IS 'Systems ranked by how long they have been running behind the latest commit. 
Used for "Top N Systems with Most Drift Time" Grafana panel.';

CREATE OR REPLACE VIEW view_systems_convergence_lag AS
WITH system_last_convergence AS (
    SELECT
        ss.hostname,
        MAX(ss.timestamp) AS last_converged_at,
        MAX(ss.timestamp) FILTER (WHERE ss.change_reason = 'config_change') AS last_config_change_at
    FROM
        system_states ss
    WHERE
        ss.timestamp >= NOW() - INTERVAL '90 days' -- Look back 90 days max
    GROUP BY
        ss.hostname
),
current_target_info AS (
    SELECT
        hostname,
        deployed_commit_timestamp,
        is_running_latest_derivation,
        last_seen
    FROM
        view_systems_current_state
)
SELECT
    slc.hostname,
    slc.last_converged_at,
    slc.last_config_change_at,
    cti.deployed_commit_timestamp,
    cti.is_running_latest_derivation,
    cti.last_seen,
    -- Time since last convergence (in hours)
    CASE WHEN slc.last_converged_at IS NOT NULL THEN
        EXTRACT(EPOCH FROM (NOW() - slc.last_converged_at)) / 3600
    ELSE
        NULL
    END AS hours_since_converged,
    -- Time since last config change (in hours)
    CASE WHEN slc.last_config_change_at IS NOT NULL THEN
        EXTRACT(EPOCH FROM (NOW() - slc.last_config_change_at)) / 3600
    ELSE
        NULL
    END AS hours_since_config_change,
    -- Status classification
    CASE WHEN cti.last_seen < NOW() - INTERVAL '4 hours' THEN
        'Offline'
    WHEN cti.is_running_latest_derivation = TRUE THEN
        'Current'
    WHEN slc.last_converged_at IS NULL THEN
        'Never Converged'
    WHEN slc.last_converged_at < NOW() - INTERVAL '7 days' THEN
        'Stale'
    ELSE
        'Behind'
    END AS convergence_status
FROM
    system_last_convergence slc
    FULL OUTER JOIN current_target_info cti ON slc.hostname = cti.hostname
ORDER BY
    hours_since_converged DESC NULLS LAST;

COMMENT ON VIEW view_systems_convergence_lag IS 'Systems ranked by time since last successful convergence to any target.
Used for "Longest Time Since Last Converged" Grafana panel.';

CREATE OR REPLACE VIEW view_recent_failed_evaluations AS
SELECT
    et.target_name AS hostname,
    et.status,
    et.started_at,
    et.completed_at,
    et.evaluation_duration_ms,
    et.attempt_count,
    c.git_commit_hash,
    c.commit_timestamp,
    f.name AS flake_name,
    f.repo_url,
    -- Time since failure
    EXTRACT(EPOCH FROM (NOW() - et.completed_at)) / 3600 AS hours_since_failure
FROM
    evaluation_targets et
    JOIN commits c ON et.commit_id = c.id
    JOIN flakes f ON c.flake_id = f.id
WHERE
    et.status = 'failed'
    AND et.completed_at >= NOW() - INTERVAL '7 days' -- Last 7 days
ORDER BY
    et.completed_at DESC;

COMMENT ON VIEW view_recent_failed_evaluations IS 'Recent evaluation failures with context for troubleshooting.
Used for "Recent Failed Evaluations" Grafana panel with drill-down capability.';

CREATE OR REPLACE VIEW view_systems_drift_time AS
SELECT
    hostname,
    current_derivation_path,
    latest_commit_derivation_path,
    deployed_commit_timestamp,
    -- Calculate how long system has been drifted
    CASE WHEN is_running_latest_derivation = FALSE
        AND deployed_commit_timestamp IS NOT NULL THEN
        EXTRACT(EPOCH FROM (NOW() - deployed_commit_timestamp)) / 3600 -- hours
    ELSE
        0
    END AS drift_hours,
    CASE WHEN is_running_latest_derivation = FALSE
        AND deployed_commit_timestamp IS NOT NULL THEN
        EXTRACT(EPOCH FROM (NOW() - deployed_commit_timestamp)) / 86400 -- days
    ELSE
        0
    END AS drift_days,
    is_running_latest_derivation,
    last_seen
FROM
    view_systems_current_state
WHERE
    is_running_latest_derivation = FALSE
ORDER BY
    drift_hours DESC;

COMMENT ON VIEW view_systems_drift_time IS 'Systems ranked by how long they have been running behind the latest commit. 
Used for "Top N Systems with Most Drift Time" Grafana panel.';

