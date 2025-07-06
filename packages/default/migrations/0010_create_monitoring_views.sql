-- View 1: Systems Current State
-- Shows each system with their current running configuration
CREATE VIEW view_systems_current_state AS
SELECT
    s.id,
    s.hostname,
    ss.derivation_path,
    ss.timestamp AS last_deployed,
    ss.derivation_path AS current_derivation_path,
    ss.primary_ip_address AS ip_address,
    ROUND(ss.uptime_secs::numeric / 86400, 1) AS uptime_days,
    ss.os,
    ss.kernel,
    ss.agent_version,
    ah.timestamp AS last_seen
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

COMMENT ON VIEW view_systems_current_state IS 'Shows each system with current config, last deployment time, and true last seen (including heartbeats)';

-- View 2: Systems Latest Evaluation
-- Shows each system with the latest evaluated derivation
CREATE VIEW view_systems_latest_commit AS
SELECT
    et.target_name AS hostname,
    et.derivation_path AS latest_derivation_path,
    et.status AS evaluation_status,
    c.commit_timestamp AS commit_timestamp,
    c.git_commit_hash AS commit_hash
FROM ( SELECT DISTINCT ON (target_name)
        target_name,
        derivation_path,
        status,
        completed_at,
        commit_id
    FROM
        evaluation_targets
    WHERE
        status = 'complete'
    ORDER BY
        target_name,
        completed_at DESC) et
    JOIN commits c ON et.commit_id = c.id
ORDER BY
    et.target_name;

COMMENT ON VIEW view_systems_latest_commit IS 'Shows each system with the latest evaluated derivation - most recent completed evaluation per system';

-- View 3: Systems Deployment Status (updated)
CREATE VIEW view_systems_deployment_status AS
SELECT
    COALESCE(current_sys.hostname, latest_eval.hostname) AS hostname,
    current_sys.current_derivation_path AS current_derivation,
    latest_eval.latest_derivation_path AS latest_derivation,
    CASE WHEN current_sys.current_derivation_path = latest_eval.latest_derivation_path THEN
        TRUE
    WHEN latest_eval.latest_derivation_path IS NULL THEN
        NULL -- No evaluation exists
    ELSE
        FALSE
    END AS is_current,
    CASE WHEN current_sys.current_derivation_path != latest_eval.latest_derivation_path
        AND latest_eval.commit_timestamp IS NOT NULL
        AND current_sys.last_deployed IS NOT NULL THEN
        EXTRACT(EPOCH FROM (current_sys.last_deployed - latest_eval.commit_timestamp))::bigint
    ELSE
        NULL
    END AS drift_duration_seconds,
    current_sys.last_seen, -- True last activity (heartbeat or state)
    current_sys.last_deployed, -- When system was last reconfigured
    current_sys.ip_address,
    current_sys.uptime_days,
    current_sys.os,
    current_sys.kernel,
    current_sys.agent_version,
    latest_eval.evaluation_status,
    latest_eval.commit_timestamp,
    latest_eval.commit_hash
FROM
    view_systems_current_state current_sys
    FULL OUTER JOIN view_systems_latest_commit latest_eval ON current_sys.hostname = latest_eval.hostname
ORDER BY
    is_current ASC NULLS LAST,
    hostname;

COMMENT ON VIEW view_systems_deployment_status IS 'Combined view with deployment status, true last seen time, and last deployment time';

-- Add index for heartbeat lookups
CREATE INDEX IF NOT EXISTS idx_agent_heartbeats_system_state_timestamp ON agent_heartbeats (system_state_id, timestamp DESC);

-- View: Systems Behind Latest Configuration
CREATE VIEW view_systems_behind AS
SELECT
    hostname,
    current_derivation,
    latest_derivation,
    drift_duration_seconds,
    last_seen
FROM
    view_systems_deployment_status
WHERE
    is_current = FALSE
ORDER BY
    drift_duration_seconds DESC;

COMMENT ON VIEW view_systems_behind IS 'Systems that are running behind the latest evaluated configuration';

-- View: Current System Inventory
CREATE VIEW view_systems_inventory AS
SELECT
    hostname,
    current_derivation_path,
    ip_address,
    uptime_days,
    os,
    kernel,
    last_seen
FROM
    view_systems_current_state
ORDER BY
    last_seen DESC;

COMMENT ON VIEW view_systems_inventory IS 'Complete inventory of all systems with current state and activity';

-- View: Evaluation Pipeline Status
CREATE VIEW view_evaluation_pipeline AS
SELECT
    hostname,
    latest_derivation_path,
    evaluation_status,
    commit_timestamp,
    commit_hash
FROM
    view_systems_latest_commit
ORDER BY
    commit_timestamp DESC;

COMMENT ON VIEW view_evaluation_pipeline IS 'Status of the evaluation pipeline showing latest evaluations per system';

-- View: Systems Requiring Attention
CREATE VIEW view_systems_attention AS
SELECT
    hostname,
    CASE WHEN is_current IS NULL THEN
        'No Evaluation'
    WHEN is_current = FALSE THEN
        'Behind Latest'
    ELSE
        'Up to Date'
    END AS status,
    last_seen,
    last_deployed,
    current_derivation,
    latest_derivation
FROM
    view_systems_deployment_status
WHERE
    is_current IS NOT TRUE
ORDER BY
    last_seen DESC;

COMMENT ON VIEW view_systems_attention IS 'Systems that need attention - either behind latest config or have no evaluation';

-- View: Evaluation Queue Status
-- Current evaluation pipeline status
CREATE VIEW view_evaluation_queue_status AS
SELECT
    target_name,
    status,
    scheduled_at,
    started_at,
    evaluation_duration_ms AS duration_ms,
    attempt_count
FROM
    evaluation_targets
WHERE
    status IN ('pending', 'queued', 'in-progress')
    OR (status IN ('complete', 'failed')
        AND completed_at > NOW() - INTERVAL '24 hours')
ORDER BY
    CASE status
    WHEN 'in-progress' THEN
        1
    WHEN 'queued' THEN
        2
    WHEN 'pending' THEN
        3
    WHEN 'failed' THEN
        4
    WHEN 'complete' THEN
        5
    END,
    scheduled_at;

COMMENT ON VIEW view_evaluation_queue_status IS 'Current evaluation pipeline status - active evaluations and recent completions';

-- View: System Heartbeat Health
-- System liveness monitoring with health indicators
CREATE VIEW view_system_heartbeat_health AS
SELECT
    sys.hostname,
    latest_hb.last_heartbeat,
    sys.last_deployed AS last_state_change,
    hb_stats.heartbeat_count_24h,
    CASE WHEN hb_stats.heartbeat_count_24h > 0 THEN
        ROUND(86400.0 / hb_stats.heartbeat_count_24h, 0)::integer
    ELSE
        NULL
    END AS avg_heartbeat_interval_seconds,
    CASE WHEN latest_hb.last_heartbeat IS NULL THEN
        'No Heartbeats'
    WHEN latest_hb.last_heartbeat > NOW() - INTERVAL '15 minutes' THEN
        'Healthy'
    WHEN latest_hb.last_heartbeat > NOW() - INTERVAL '1 hour' THEN
        'Warning'
    WHEN latest_hb.last_heartbeat > NOW() - INTERVAL '4 hours' THEN
        'Critical'
    ELSE
        'Offline'
    END AS status
FROM
    view_systems_current_state sys
    LEFT JOIN (
        -- Get the most recent heartbeat per system
        SELECT DISTINCT ON (ss.hostname)
            ss.hostname,
            ah.timestamp AS last_heartbeat
        FROM
            system_states ss
            JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
        ORDER BY
            ss.hostname,
            ah.timestamp DESC) latest_hb ON sys.hostname = latest_hb.hostname
    LEFT JOIN (
        -- Calculate heartbeat frequency over last 24 hours
        SELECT
            ss.hostname,
            COUNT(ah.id) AS heartbeat_count_24h
        FROM
            system_states ss
            JOIN agent_heartbeats ah ON ah.system_state_id = ss.id
        WHERE
            ah.timestamp > NOW() - INTERVAL '24 hours'
        GROUP BY
            ss.hostname) hb_stats ON sys.hostname = hb_stats.hostname
ORDER BY
    CASE WHEN latest_hb.last_heartbeat IS NULL THEN
        1
    WHEN latest_hb.last_heartbeat <= NOW() - INTERVAL '4 hours' THEN
        2
    WHEN latest_hb.last_heartbeat <= NOW() - INTERVAL '1 hour' THEN
        3
    WHEN latest_hb.last_heartbeat <= NOW() - INTERVAL '15 minutes' THEN
        4
    ELSE
        5
    END,
    latest_hb.last_heartbeat DESC NULLS LAST;

COMMENT ON VIEW view_system_heartbeat_health IS 'System liveness monitoring with health status based on heartbeat patterns';

-- THIS
WITH latest_commit AS (
    -- Get THE latest commit across all flakes
    SELECT
        id AS latest_commit_id,
        git_commit_hash AS latest_commit_hash,
        commit_timestamp AS latest_commit_timestamp
    FROM
        commits
    ORDER BY
        commit_timestamp DESC
    LIMIT 1
),
all_systems AS (
    -- Get all unique systems
    SELECT DISTINCT
        target_name AS hostname
    FROM
        evaluation_targets
),
latest_commit_evaluations AS (
    -- Get evaluations for the latest commit
    SELECT
        target_name AS hostname,
        derivation_path AS latest_derivation_path,
        status AS evaluation_status
    FROM
        evaluation_targets et
        CROSS JOIN latest_commit lc
    WHERE
        et.commit_id = lc.latest_commit_id
        AND et.status = 'complete'
),
current_system_states AS (
    -- Get current derivation for each system
    SELECT DISTINCT ON (hostname)
        hostname,
        derivation_path AS current_derivation_path
    FROM
        system_states
    ORDER BY
        hostname,
        timestamp DESC
)
SELECT
    s.hostname,
    css.current_derivation_path,
    lce.latest_derivation_path,
    lce.evaluation_status,
    lc.latest_commit_timestamp AS commit_timestamp,
    lc.latest_commit_hash AS commit_hash
FROM
    all_systems s
    CROSS JOIN latest_commit lc
    LEFT JOIN latest_commit_evaluations lce ON s.hostname = lce.hostname
    LEFT JOIN current_system_states css ON s.hostname = css.hostname
ORDER BY
    s.hostname;

