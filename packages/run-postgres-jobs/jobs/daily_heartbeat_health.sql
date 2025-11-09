INSERT INTO daily_heartbeat_health (snapshot_date, total_systems, systems_healthy, systems_warning, systems_critical, systems_offline, systems_no_heartbeats, avg_heartbeat_interval_minutes, total_heartbeats_24h, created_at)
SELECT
    CURRENT_DATE AS snapshot_date,
    COUNT(*) AS total_systems,
    COUNT(*) FILTER (WHERE heartbeat_status = 'Healthy') AS systems_healthy,
    COUNT(*) FILTER (WHERE heartbeat_status = 'Warning') AS systems_warning,
    COUNT(*) FILTER (WHERE heartbeat_status = 'Critical') AS systems_critical,
    COUNT(*) FILTER (WHERE heartbeat_status = 'Offline') AS systems_offline,
    0 AS systems_no_heartbeats,
    ROUND(AVG(minutes_since_last_activity), 2) AS avg_heartbeat_interval_minutes,
    (
        SELECT
            COUNT(*)
        FROM
            agent_heartbeats
        WHERE
            timestamp >= CURRENT_DATE - INTERVAL '1 day') AS total_heartbeats_24h,
    NOW() AS created_at
FROM
    view_system_heartbeat_status
ON CONFLICT (snapshot_date)
    DO UPDATE SET
        total_systems = EXCLUDED.total_systems,
        systems_healthy = EXCLUDED.systems_healthy,
        systems_warning = EXCLUDED.systems_warning,
        systems_critical = EXCLUDED.systems_critical,
        systems_offline = EXCLUDED.systems_offline,
        systems_no_heartbeats = EXCLUDED.systems_no_heartbeats,
        avg_heartbeat_interval_minutes = EXCLUDED.avg_heartbeat_interval_minutes,
        total_heartbeats_24h = EXCLUDED.total_heartbeats_24h,
        created_at = NOW();

