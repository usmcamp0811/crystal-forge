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

