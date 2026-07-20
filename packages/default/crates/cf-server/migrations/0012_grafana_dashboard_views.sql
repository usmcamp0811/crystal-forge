CREATE OR REPLACE VIEW view_systems_current_state AS
SELECT
    s.id AS system_id,
    s.hostname,
    -- Current deployment info (from latest flake commit)
    vslfc.repo_url,
    vslfc.git_commit_hash AS deployed_commit_hash,
    vslfc.commit_timestamp AS deployed_commit_timestamp,
    -- System state info (from latest system state)
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
    GREATEST (COALESCE(lss.timestamp, '1970-01-01'::timestamp), COALESCE(lhb.last_heartbeat, '1970-01-01'::timestamp)) AS last_seen
FROM
    systems s -- Start with ALL systems as the base
    LEFT JOIN view_systems_latest_flake_commit vslfc ON s.hostname = vslfc.hostname
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
            timestamp DESC) lss ON s.hostname = lss.hostname
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
            ah.timestamp DESC) lhb ON s.hostname = lhb.hostname
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
            et.status = 'complete') latest_eval ON s.hostname = latest_eval.hostname
ORDER BY
    s.hostname;

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

CREATE TABLE IF NOT EXISTS daily_drift_snapshots (
    snapshot_date date,
    hostname varchar(255),
    drift_hours numeric,
    is_behind boolean,
    created_at timestamptz DEFAULT NOW(),
    PRIMARY KEY (snapshot_date, hostname)
);

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
        0 -- Up-to-date systems show 0 drift
    END AS drift_hours,
    CASE WHEN is_running_latest_derivation = FALSE
        AND deployed_commit_timestamp IS NOT NULL THEN
        EXTRACT(EPOCH FROM (NOW() - deployed_commit_timestamp)) / 86400 -- days
    ELSE
        0 -- Up-to-date systems show 0 drift
    END AS drift_days,
    is_running_latest_derivation,
    last_seen
FROM
    view_systems_current_state
    -- REMOVED: WHERE is_running_latest_derivation = FALSE
ORDER BY
    drift_hours DESC;

COMMENT ON VIEW view_systems_drift_time IS 'All systems with drift time calculations. 
Up-to-date systems show 0 drift hours/days, behind systems show actual drift.
Used for "Top N Systems with Most Drift Time" Grafana panel and daily_drift_snapshots job.';

-- 1. SYSTEM COMPLIANCE TIME SERIES
-- Tracks number of systems that are up-to-date vs behind vs offline
CREATE TABLE IF NOT EXISTS daily_compliance_snapshots (
    snapshot_date date PRIMARY KEY,
    total_systems integer NOT NULL,
    systems_up_to_date integer NOT NULL,
    systems_behind integer NOT NULL,
    systems_no_evaluation integer NOT NULL,
    systems_offline integer NOT NULL,
    systems_never_seen integer NOT NULL,
    compliance_percentage numeric(5, 2) NOT NULL,
    created_at timestamptz DEFAULT NOW()
);

-- 3. EVALUATION PIPELINE HEALTH TIME SERIES
-- Tracks success/failure rates of evaluations
CREATE TABLE IF NOT EXISTS daily_evaluation_health (
    snapshot_date date PRIMARY KEY,
    total_evaluations integer NOT NULL,
    successful_evaluations integer NOT NULL,
    failed_evaluations integer NOT NULL,
    pending_evaluations integer NOT NULL,
    avg_evaluation_duration_ms integer,
    max_evaluation_duration_ms integer,
    success_rate_percentage numeric(5, 2) NOT NULL,
    evaluations_with_retries integer NOT NULL,
    created_at timestamptz DEFAULT NOW()
);

-- 4. SYSTEM HEARTBEAT HEALTH TIME SERIES
-- Tracks system connectivity and health patterns
CREATE TABLE IF NOT EXISTS daily_heartbeat_health (
    snapshot_date date PRIMARY KEY,
    total_systems integer NOT NULL,
    systems_healthy integer NOT NULL, -- heartbeat < 15 min
    systems_warning integer NOT NULL, -- heartbeat 15min - 1hr
    systems_critical integer NOT NULL, -- heartbeat 1hr - 4hr
    systems_offline integer NOT NULL, -- heartbeat > 4hr
    systems_no_heartbeats integer NOT NULL, -- never had heartbeat
    avg_heartbeat_interval_minutes numeric(8, 2),
    total_heartbeats_24h integer NOT NULL,
    created_at timestamptz DEFAULT NOW()
);

-- 5. SECURITY POSTURE TIME SERIES
-- Tracks security-relevant system configurations
CREATE TABLE IF NOT EXISTS daily_security_posture (
    snapshot_date date PRIMARY KEY,
    total_systems integer NOT NULL,
    systems_with_tpm integer NOT NULL,
    systems_secure_boot integer NOT NULL,
    systems_fips_mode integer NOT NULL,
    systems_selinux_enforcing integer NOT NULL,
    systems_agent_compatible integer NOT NULL,
    unique_agent_versions integer NOT NULL,
    outdated_agent_count integer NOT NULL,
    created_at timestamptz DEFAULT NOW()
);

-- 6. COMMIT VELOCITY AND DEPLOYMENT LAG TIME SERIES
-- Tracks how quickly commits are being evaluated and deployed
CREATE TABLE IF NOT EXISTS daily_deployment_velocity (
    snapshot_date date PRIMARY KEY,
    new_commits_today integer NOT NULL,
    commits_evaluated_today integer NOT NULL,
    commits_deployed_today integer NOT NULL,
    avg_eval_to_deploy_hours numeric(8, 2),
    max_eval_to_deploy_hours numeric(8, 2),
    systems_updated_today integer NOT NULL,
    fastest_deployment_minutes integer,
    slowest_deployment_hours numeric(8, 2),
    created_at timestamptz DEFAULT NOW()
);

-- ============================================================================
-- HELPER VIEWS FOR TIME SERIES ANALYSIS
-- ============================================================================
-- View: 7-day compliance trend
CREATE OR REPLACE VIEW view_compliance_trend_7d AS
SELECT
    snapshot_date,
    total_systems,
    systems_up_to_date,
    systems_behind,
    compliance_percentage,
    compliance_percentage - LAG(compliance_percentage) OVER (ORDER BY snapshot_date) AS daily_change
FROM
    daily_compliance_snapshots
WHERE
    snapshot_date >= CURRENT_DATE - INTERVAL '7 days'
ORDER BY
    snapshot_date;

-- View: Security posture trends
CREATE OR REPLACE VIEW view_security_trend_30d AS
SELECT
    snapshot_date,
    total_systems,
    ROUND(systems_with_tpm * 100.0 / total_systems, 1) AS tpm_percentage,
    ROUND(systems_secure_boot * 100.0 / total_systems, 1) AS secure_boot_percentage,
    ROUND(systems_fips_mode * 100.0 / total_systems, 1) AS fips_percentage,
    ROUND(systems_selinux_enforcing * 100.0 / total_systems, 1) AS selinux_percentage,
    unique_agent_versions,
    outdated_agent_count
FROM
    daily_security_posture
WHERE
    snapshot_date >= CURRENT_DATE - INTERVAL '30 days'
ORDER BY
    snapshot_date;

-- View: Deployment velocity trends
CREATE OR REPLACE VIEW view_velocity_trend_14d AS
SELECT
    snapshot_date,
    new_commits_today,
    commits_evaluated_today,
    commits_deployed_today,
    ROUND(avg_eval_to_deploy_hours, 2) AS avg_eval_to_deploy_hours,
    systems_updated_today,
    ROUND(commits_deployed_today * 100.0 / NULLIF (commits_evaluated_today, 0), 1) AS deployment_rate_percentage
FROM
    daily_deployment_velocity
WHERE
    snapshot_date >= CURRENT_DATE - INTERVAL '14 days'
ORDER BY
    snapshot_date;

