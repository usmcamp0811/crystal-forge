INSERT INTO daily_drift_snapshots (snapshot_date, hostname, drift_hours, is_behind)
SELECT
    CURRENT_DATE,
    hostname,
    drift_hours,
    (drift_hours > 0) AS is_behind
FROM
    view_systems_drift_time
ON CONFLICT (snapshot_date,
    hostname)
    DO UPDATE SET
        drift_hours = EXCLUDED.drift_hours,
        is_behind = EXCLUDED.is_behind,
        created_at = NOW();

