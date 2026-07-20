CREATE OR REPLACE VIEW view_system_heartbeat_health AS
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

