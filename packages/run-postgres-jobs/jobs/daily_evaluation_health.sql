INSERT INTO daily_evaluation_health (snapshot_date, total_evaluations, successful_evaluations, failed_evaluations, pending_evaluations, avg_evaluation_duration_ms, max_evaluation_duration_ms, success_rate_percentage, evaluations_with_retries, created_at)
SELECT
    CURRENT_DATE AS snapshot_date,
    COUNT(*) AS total_evaluations,
    COUNT(*) FILTER (WHERE status = 'complete') AS successful_evaluations,
    COUNT(*) FILTER (WHERE status = 'failed') AS failed_evaluations,
    COUNT(*) FILTER (WHERE status IN ('pending', 'queued', 'in-progress')) AS pending_evaluations,
    AVG(evaluation_duration_ms)::integer AS avg_evaluation_duration_ms,
    MAX(evaluation_duration_ms) AS max_evaluation_duration_ms,
    COALESCE(ROUND(COUNT(*) FILTER (WHERE status = 'complete') * 100.0 / NULLIF (COUNT(*) FILTER (WHERE status IN ('complete', 'failed')), 0), 2), 0) AS success_rate_percentage,
    COUNT(*) FILTER (WHERE attempt_count > 1) AS evaluations_with_retries,
    NOW() AS created_at
FROM
    evaluation_targets
WHERE
    completed_at >= CURRENT_DATE - INTERVAL '1 day'
    OR status IN ('pending', 'queued', 'in-progress')
ON CONFLICT (snapshot_date)
    DO UPDATE SET
        total_evaluations = EXCLUDED.total_evaluations,
        successful_evaluations = EXCLUDED.successful_evaluations,
        failed_evaluations = EXCLUDED.failed_evaluations,
        pending_evaluations = EXCLUDED.pending_evaluations,
        avg_evaluation_duration_ms = EXCLUDED.avg_evaluation_duration_ms,
        max_evaluation_duration_ms = EXCLUDED.max_evaluation_duration_ms,
        success_rate_percentage = EXCLUDED.success_rate_percentage,
        evaluations_with_retries = EXCLUDED.evaluations_with_retries,
        created_at = NOW();

