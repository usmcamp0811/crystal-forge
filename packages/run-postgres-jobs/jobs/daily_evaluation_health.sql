INSERT INTO daily_evaluation_health (snapshot_date, total_evaluations, successful_evaluations, failed_evaluations, pending_evaluations, avg_evaluation_duration_ms, max_evaluation_duration_ms, success_rate_percentage, evaluations_with_retries, created_at)
SELECT
    CURRENT_DATE AS snapshot_date,
    COUNT(*) AS total_evaluations,
    COUNT(*) FILTER (WHERE ds.name = 'complete') AS successful_evaluations,
    COUNT(*) FILTER (WHERE ds.name = 'failed') AS failed_evaluations,
    COUNT(*) FILTER (WHERE ds.name IN ('pending', 'queued', 'in-progress')) AS pending_evaluations,
    AVG(d.evaluation_duration_ms)::integer AS avg_evaluation_duration_ms,
    MAX(d.evaluation_duration_ms) AS max_evaluation_duration_ms,
    COALESCE(ROUND(COUNT(*) FILTER (WHERE ds.name = 'complete') * 100.0 / NULLIF (COUNT(*) FILTER (WHERE ds.name IN ('complete', 'failed')), 0), 2), 0) AS success_rate_percentage,
    COUNT(*) FILTER (WHERE d.attempt_count > 1) AS evaluations_with_retries,
    NOW() AS created_at
FROM
    derivations d
    JOIN derivation_statuses ds ON d.status_id = ds.id
WHERE
    d.completed_at >= CURRENT_DATE - INTERVAL '1 day'
    OR ds.name IN ('pending', 'queued', 'in-progress')
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

