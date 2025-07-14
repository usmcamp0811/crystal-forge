-- 2. SYSTEM DRIFT TIME SERIES
-- Tracks how long systems have been drifting from latest config
-- (You already have daily_drift_snapshots table, here's the query)
INSERT INTO daily_drift_snapshots (snapshot_date, hostname, drift_hours, is_behind)
SELECT
    CURRENT_DATE AS snapshot_date,
    hostname,
    drift_hours,
    (is_running_latest_derivation = FALSE) AS is_behind
FROM
    view_systems_drift_time
ON CONFLICT (snapshot_date,
    hostname)
    DO UPDATE SET
        drift_hours = EXCLUDED.drift_hours,
        is_behind = EXCLUDED.is_behind,
        created_at = NOW();

