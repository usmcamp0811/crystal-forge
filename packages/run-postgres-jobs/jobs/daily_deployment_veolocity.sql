INSERT INTO daily_deployment_velocity (snapshot_date, new_commits_today, commits_evaluated_today, commits_deployed_today, avg_eval_to_deploy_hours, max_eval_to_deploy_hours, systems_updated_today, fastest_deployment_minutes, slowest_deployment_hours)
WITH todays_activity AS (
    SELECT
        c.id AS commit_id,
        c.commit_timestamp,
        MIN(et.completed_at) AS first_evaluation_completed,
        MIN(ss.timestamp) FILTER (WHERE ss.change_reason = 'config_change'
            AND ss.timestamp >= c.commit_timestamp) AS first_deployment
    FROM
        commits c
    LEFT JOIN evaluation_targets et ON c.id = et.commit_id
        AND et.status = 'complete'
    LEFT JOIN system_states ss ON ss.derivation_path = et.derivation_path
WHERE
    c.commit_timestamp >= CURRENT_DATE - INTERVAL '1 day'
GROUP BY
    c.id,
    c.commit_timestamp),
deployment_timings AS (
    SELECT
        EXTRACT(EPOCH FROM (first_deployment - first_evaluation_completed)) / 3600 AS eval_to_deploy_hours,
        EXTRACT(EPOCH FROM (first_deployment - commit_timestamp)) / 60 AS commit_to_deploy_minutes
    FROM
        todays_activity
    WHERE
        first_evaluation_completed IS NOT NULL
        AND first_deployment IS NOT NULL
)
SELECT
    CURRENT_DATE AS snapshot_date,
    (
        SELECT
            COUNT(*)
        FROM
            commits
        WHERE
            commit_timestamp >= CURRENT_DATE - INTERVAL '1 day') AS new_commits_today,
    (
        SELECT
            COUNT(DISTINCT commit_id)
        FROM
            evaluation_targets
        WHERE
            completed_at >= CURRENT_DATE - INTERVAL '1 day'
            AND status = 'complete') AS commits_evaluated_today,
    COUNT(*) FILTER (WHERE first_deployment IS NOT NULL) AS commits_deployed_today,
    AVG(eval_to_deploy_hours) AS avg_eval_to_deploy_hours,
    MAX(eval_to_deploy_hours) AS max_eval_to_deploy_hours,
    (
        SELECT
            COUNT(DISTINCT hostname)
        FROM
            system_states
        WHERE
            timestamp >= CURRENT_DATE - INTERVAL '1 day'
            AND change_reason = 'config_change') AS systems_updated_today,
    MIN(commit_to_deploy_minutes)::integer AS fastest_deployment_minutes,
    MAX(commit_to_deploy_minutes / 60.0) AS slowest_deployment_hours
FROM
    todays_activity ta
    LEFT JOIN deployment_timings dt ON TRUE
ON CONFLICT (snapshot_date)
    DO UPDATE SET
        new_commits_today = EXCLUDED.new_commits_today,
        commits_evaluated_today = EXCLUDED.commits_evaluated_today,
        commits_deployed_today = EXCLUDED.commits_deployed_today,
        avg_eval_to_deploy_hours = EXCLUDED.avg_eval_to_deploy_hours,
        max_eval_to_deploy_hours = EXCLUDED.max_eval_to_deploy_hours,
        systems_updated_today = EXCLUDED.systems_updated_today,
        fastest_deployment_minutes = EXCLUDED.fastest_deployment_minutes,
        slowest_deployment_hours = EXCLUDED.slowest_deployment_hours,
        created_at = NOW();

