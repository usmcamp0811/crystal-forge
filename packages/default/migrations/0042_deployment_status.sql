CREATE OR REPLACE VIEW view_deployment_status AS
SELECT
    COUNT(*) AS count,
    CASE update_status
    WHEN 'up_to_date' THEN
        '🟢 Up to Date'
    WHEN 'behind' THEN
        '🔴 Behind'
    WHEN 'evaluation_failed' THEN
        '🟤 Evaluation Failed'
    WHEN 'no_evaluation' THEN
        '⚪ No Evaluation'
    WHEN 'no_deployment' THEN
        '⚪ No Deployment'
    WHEN 'never_seen' THEN
        '⚫ Never Seen'
    WHEN 'unknown' THEN
        '❓ Unknown'
    ELSE
        '❓ Unknown'
    END AS status_display
FROM
    view_systems_status_table
GROUP BY
    update_status
ORDER BY
    CASE update_status
    WHEN 'up_to_date' THEN
        1 -- 🟢 Up to Date
    WHEN 'behind' THEN
        2 -- 🔴 Behind (outdated)
    WHEN 'evaluation_failed' THEN
        3 -- 🟤 Evaluation Failed (stuck)
    WHEN 'no_evaluation' THEN
        4 -- ⚪ No Evaluation (no data)
    WHEN 'no_deployment' THEN
        5 -- ⚪ No Deployment (no data)
    WHEN 'never_seen' THEN
        6 -- ⚫ Never Seen (offline)
    WHEN 'unknown' THEN
        7 -- ❓ Unknown
    ELSE
        8
    END;

