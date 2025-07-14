-- Nightly query to populate compliance snapshots
INSERT INTO daily_compliance_snapshots (snapshot_date, total_systems, systems_up_to_date, systems_behind, systems_no_evaluation, systems_offline, systems_never_seen, compliance_percentage)
SELECT
    CURRENT_DATE AS snapshot_date,
    COUNT(*) AS total_systems,
    COUNT(*) FILTER (WHERE is_running_latest_derivation = TRUE) AS systems_up_to_date,
    COUNT(*) FILTER (WHERE is_running_latest_derivation = FALSE) AS systems_behind,
    COUNT(*) FILTER (WHERE is_running_latest_derivation IS NULL) AS systems_no_evaluation,
    COUNT(*) FILTER (WHERE last_seen < NOW() - INTERVAL '4 hours') AS systems_offline,
    COUNT(*) FILTER (WHERE last_seen IS NULL) AS systems_never_seen,
    ROUND(COUNT(*) FILTER (WHERE is_running_latest_derivation = TRUE) * 100.0 / NULLIF (COUNT(*), 0), 2) AS compliance_percentage
FROM
    view_systems_current_state
ON CONFLICT (snapshot_date)
    DO UPDATE SET
        total_systems = EXCLUDED.total_systems,
        systems_up_to_date = EXCLUDED.systems_up_to_date,
        systems_behind = EXCLUDED.systems_behind,
        systems_no_evaluation = EXCLUDED.systems_no_evaluation,
        systems_offline = EXCLUDED.systems_offline,
        systems_never_seen = EXCLUDED.systems_never_seen,
        compliance_percentage = EXCLUDED.compliance_percentage,
        created_at = NOW();

