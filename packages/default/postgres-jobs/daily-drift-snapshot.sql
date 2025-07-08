INSERT INTO daily_drift_snapshots (snapshot_date, hostname, drift_hours, is_behind)
SELECT
    CURRENT_DATE,
    hostname,
    drift_hours,
    (drift_hours > 0) AS is_behind
FROM
    view_systems_drift_time;

