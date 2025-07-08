CREATE OR REPLACE VIEW view_systems_summary AS
SELECT
    (
        SELECT
            COUNT(*)
        FROM
            view_systems_current_state) AS "Total Systems",
    (
        SELECT
            COUNT(*)
        FROM
            view_systems_current_state
        WHERE
            is_running_latest_derivation = TRUE) AS "Up to Date",
    (
        SELECT
            COUNT(*)
        FROM
            view_systems_current_state
        WHERE
            is_running_latest_derivation = FALSE) AS "Behind Latest",
    (
        SELECT
            COUNT(*)
        FROM
            view_systems_current_state
        WHERE
            last_seen < NOW() - INTERVAL '15 minutes') AS "No Recent Heartbeat";

COMMENT ON VIEW view_systems_summary IS 'Summary view providing key system metrics:
- "Total Systems": total systems reporting
- "Up to Date": systems running latest derivation
- "Behind Latest": systems not on latest derivation
- "No Recent Heartbeat": systems without heartbeat in last 15 minutes.';

CREATE OR REPLACE VIEW view_systems_status_table AS
SELECT
    hostname,
    -- Determine status symbol
    CASE WHEN current_derivation_path IS NULL THEN
        '⚫' -- No System State
    WHEN latest_commit_derivation_path IS NULL THEN
        '⚪' -- No Evaluation
    WHEN current_derivation_path = latest_commit_derivation_path THEN
        '🟢' -- Current
    WHEN latest_commit_evaluation_status != 'complete' THEN
        '🟡' -- Eval Pending
    ELSE
        '🔴' -- Behind
    END AS status,
    CONCAT(EXTRACT(EPOCH FROM NOW() - last_seen)::int / 60, 'm ago') AS last_seen,
    agent_version AS version,
    uptime_days || 'd' AS uptime,
    ip_address
FROM
    view_systems_current_state
ORDER BY
    CASE WHEN current_derivation_path = latest_commit_derivation_path THEN
        1 -- 🟢
    WHEN latest_commit_evaluation_status != 'complete' THEN
        2 -- 🟡
    ELSE
        3 -- 🔴/others
    END,
    last_seen;

COMMENT ON VIEW view_systems_status_table IS 'Provides system status with hostname, status symbol, last seen time, agent version, uptime, and IP for Grafana dashboards.';

