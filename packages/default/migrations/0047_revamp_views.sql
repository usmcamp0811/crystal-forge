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

