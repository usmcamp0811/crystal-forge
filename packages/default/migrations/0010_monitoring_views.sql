-- Migration: Create monitoring views
-- File: packages/default/migrations/XXXX_create_monitoring_views.sql
-- View 1: Systems Current State
-- Shows each system with their current running configuration
CREATE VIEW view_systems_current_state AS
SELECT
    ss.hostname,
    ss.derivation_path AS current_derivation_path,
    ss.timestamp AS last_deployed, -- When system state last changed
    COALESCE(hb.last_heartbeat, ss.timestamp) AS last_seen, -- True last activity
    ss.primary_ip_address AS ip_address,
    ROUND(ss.uptime_secs::numeric / 86400, 1) AS uptime_days,
    ss.os,
    ss.kernel,
    ss.agent_version
FROM ( SELECT DISTINCT ON (hostname)
        hostname,
        derivation_path,
        timestamp,
        primary_ip_address,
        uptime_secs,
        os,
        kernel,
        agent_version,
        id
    FROM
        system_states
    ORDER BY
        hostname,
        timestamp DESC) ss
    LEFT JOIN (
        -- Get the most recent heartbeat for each system
        SELECT DISTINCT ON (ss2.hostname)
            ss2.hostname,
            ah.timestamp AS last_heartbeat
        FROM
            system_states ss2
            JOIN agent_heartbeats ah ON ah.system_state_id = ss2.id
        ORDER BY
            ss2.hostname,
            ah.timestamp DESC) hb ON ss.hostname = hb.hostname
ORDER BY
    ss.hostname;

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

