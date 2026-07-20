CREATE OR REPLACE VIEW view_critical_systems AS
SELECT
    hostname,
    CASE WHEN last_seen::timestamptz > NOW() - INTERVAL '4 hours'
        AND last_seen::timestamptz <= NOW() - INTERVAL '1 hour' THEN
        'Critical'
    ELSE
        'Offline'
    END AS status,
    ROUND(
        CASE WHEN last_seen IS NOT NULL
            AND last_seen != 'Unknown' THEN
            EXTRACT(EPOCH FROM (NOW() - last_seen::timestamptz)) / 3600.0
        ELSE
            NULL
        END, 1) AS hours_ago,
    ip_address AS ip,
    agent_version AS version
FROM
    view_systems_status_table
WHERE
    -- Critical: last seen between 1-4 hours ago
    (last_seen::timestamptz > NOW() - INTERVAL '4 hours'
        AND last_seen::timestamptz <= NOW() - INTERVAL '1 hour')
    OR
    -- Offline: last seen more than 4 hours ago or never seen
    (last_seen::timestamptz <= NOW() - INTERVAL '4 hours'
        OR last_seen IS NULL
        OR last_seen = 'Unknown')
ORDER BY
    CASE WHEN last_seen::timestamptz <= NOW() - INTERVAL '4 hours'
        OR last_seen IS NULL
        OR last_seen = 'Unknown' THEN
        1
    ELSE
        2
    END,
    hours_ago DESC NULLS LAST;

