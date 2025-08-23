CREATE OR REPLACE VIEW view_fleet_health_status AS
SELECT
    health_status,
    COUNT(*) AS count,
FROM (
    SELECT
        hostname,
        CASE WHEN last_seen::timestamptz > NOW() - INTERVAL '15 minutes' THEN
            'Healthy'
        WHEN last_seen::timestamptz > NOW() - INTERVAL '1 hour' THEN
            'Warning'
        WHEN last_seen::timestamptz > NOW() - INTERVAL '4 hours' THEN
            'Critical'
        ELSE
            'Offline'
        END AS health_status
    FROM
        view_systems_status_table
    WHERE
        last_seen IS NOT NULL
        AND last_seen != 'Unknown') health_data
GROUP BY
    health_status
ORDER BY
    CASE health_status
    WHEN 'Healthy' THEN
        1
    WHEN 'Warning' THEN
        2
    WHEN 'Critical' THEN
        3
    WHEN 'Offline' THEN
        4
    ELSE
        5
    END;

