CREATE OR REPLACE VIEW public.view_derivation_stats AS
WITH overall_stats AS (
    SELECT
        -- Overall counts
        COUNT(*) AS total_derivations,
        COUNT(*) FILTER (WHERE derivation_type = 'nixos') AS nixos_systems,
        COUNT(*) FILTER (WHERE derivation_type = 'package') AS packages,
        -- Status breakdown
        COUNT(*) FILTER (WHERE ds.name = 'pending') AS pending,
        COUNT(*) FILTER (WHERE ds.name = 'queued') AS queued,
        COUNT(*) FILTER (WHERE ds.name = 'dry-run-pending') AS dry_run_pending,
        COUNT(*) FILTER (WHERE ds.name = 'dry-run-in-progress') AS dry_run_in_progress,
        COUNT(*) FILTER (WHERE ds.name = 'dry-run-complete') AS dry_run_complete,
        COUNT(*) FILTER (WHERE ds.name = 'dry-run-failed') AS dry_run_failed,
        COUNT(*) FILTER (WHERE ds.name = 'build-pending') AS build_pending,
        COUNT(*) FILTER (WHERE ds.name = 'build-in-progress') AS build_in_progress,
        COUNT(*) FILTER (WHERE ds.name = 'build-complete') AS build_complete,
        COUNT(*) FILTER (WHERE ds.name = 'build-failed') AS build_failed,
        COUNT(*) FILTER (WHERE ds.name = 'complete') AS complete,
        COUNT(*) FILTER (WHERE ds.name = 'failed') AS failed,
        -- Terminal vs non-terminal
        COUNT(*) FILTER (WHERE ds.is_terminal = TRUE) AS terminal_derivations,
        COUNT(*) FILTER (WHERE ds.is_terminal = FALSE) AS active_derivations,
        -- Success vs failure (among terminal)
        COUNT(*) FILTER (WHERE ds.is_terminal = TRUE
            AND ds.is_success = TRUE) AS successful_derivations,
        COUNT(*) FILTER (WHERE ds.is_terminal = TRUE
            AND ds.is_success = FALSE) AS failed_derivations,
        -- Derivation paths (successful builds)
        COUNT(*) FILTER (WHERE derivation_path IS NOT NULL) AS derivations_with_paths,
        COUNT(*) FILTER (WHERE derivation_path IS NULL) AS derivations_without_paths,
        -- Commit relationships
        COUNT(*) FILTER (WHERE commit_id IS NOT NULL) AS derivations_with_commits,
        COUNT(*) FILTER (WHERE commit_id IS NULL) AS standalone_derivations,
        -- Attempt tracking
        AVG(attempt_count) AS avg_attempt_count,
        MAX(attempt_count) AS max_attempt_count,
        COUNT(*) FILTER (WHERE attempt_count > 1) AS derivations_with_retries,
        -- Timing stats (for completed derivations)
        AVG(evaluation_duration_ms) FILTER (WHERE evaluation_duration_ms IS NOT NULL) AS avg_evaluation_duration_ms,
        MAX(evaluation_duration_ms) AS max_evaluation_duration_ms,
        -- Age analysis
        COUNT(*) FILTER (WHERE scheduled_at >= NOW() - INTERVAL '1 hour') AS scheduled_last_hour,
        COUNT(*) FILTER (WHERE scheduled_at >= NOW() - INTERVAL '24 hours') AS scheduled_last_day,
        COUNT(*) FILTER (WHERE scheduled_at >= NOW() - INTERVAL '7 days') AS scheduled_last_week,
        COUNT(*) FILTER (WHERE completed_at >= NOW() - INTERVAL '1 hour') AS completed_last_hour,
        COUNT(*) FILTER (WHERE completed_at >= NOW() - INTERVAL '24 hours') AS completed_last_day,
        COUNT(*) FILTER (WHERE completed_at >= NOW() - INTERVAL '7 days') AS completed_last_week
    FROM
        public.derivations d
        JOIN public.derivation_statuses ds ON d.status_id = ds.id
),
type_breakdown AS (
    SELECT
        'nixos' AS derivation_type,
        COUNT(*) AS count,
        COUNT(*) FILTER (WHERE ds.is_terminal = TRUE
                AND ds.is_success = TRUE) AS successful,
            COUNT(*) FILTER (WHERE ds.is_terminal = TRUE
                AND ds.is_success = FALSE) AS failed,
            COUNT(*) FILTER (WHERE ds.is_terminal = FALSE) AS in_progress,
            AVG(attempt_count) AS avg_attempts,
            AVG(evaluation_duration_ms) FILTER (WHERE evaluation_duration_ms IS NOT NULL) AS avg_duration_ms
        FROM
            public.derivations d
        JOIN public.derivation_statuses ds ON d.status_id = ds.id
    WHERE
        d.derivation_type = 'nixos'
    UNION ALL
    SELECT
        'package' AS derivation_type,
        COUNT(*) AS count,
        COUNT(*) FILTER (WHERE ds.is_terminal = TRUE
            AND ds.is_success = TRUE) AS successful,
        COUNT(*) FILTER (WHERE ds.is_terminal = TRUE
            AND ds.is_success = FALSE) AS failed,
        COUNT(*) FILTER (WHERE ds.is_terminal = FALSE) AS in_progress,
        AVG(attempt_count) AS avg_attempts,
        AVG(evaluation_duration_ms) FILTER (WHERE evaluation_duration_ms IS NOT NULL) AS avg_duration_ms
    FROM
        public.derivations d
        JOIN public.derivation_statuses ds ON d.status_id = ds.id
    WHERE
        d.derivation_type = 'package'
),
status_summary AS (
    SELECT
        ds.name AS status_name,
        ds.description AS status_description,
        ds.is_terminal,
        ds.is_success,
        COUNT(*) AS derivation_count,
        COUNT(*) FILTER (WHERE d.derivation_type = 'nixos') AS nixos_count,
        COUNT(*) FILTER (WHERE d.derivation_type = 'package') AS package_count,
        AVG(d.attempt_count) AS avg_attempts,
        MIN(d.scheduled_at) AS oldest_scheduled,
        MAX(d.scheduled_at) AS newest_scheduled
    FROM
        public.derivations d
        JOIN public.derivation_statuses ds ON d.status_id = ds.id
    GROUP BY
        ds.id,
        ds.name,
        ds.description,
        ds.is_terminal,
        ds.is_success
    ORDER BY
        ds.display_order
)
SELECT
    -- Overall metrics
    os.total_derivations,
    os.nixos_systems,
    os.packages,
    -- Status counts
    os.pending,
    os.queued,
    os.dry_run_pending,
    os.dry_run_in_progress,
    os.dry_run_complete,
    os.dry_run_failed,
    os.build_pending,
    os.build_in_progress,
    os.build_complete,
    os.build_failed,
    os.complete,
    os.failed,
    -- Success metrics
    ROUND((os.successful_derivations::numeric / NULLIF (os.terminal_derivations, 0)) * 100, 2) AS success_rate_percent,
    os.successful_derivations,
    os.failed_derivations,
    os.active_derivations,
    -- Build progress
    os.derivations_with_paths,
    os.derivations_without_paths,
    ROUND((os.derivations_with_paths::numeric / NULLIF (os.total_derivations, 0)) * 100, 2) AS build_completion_percent,
    -- Retry metrics
    ROUND(os.avg_attempt_count, 2) AS avg_attempt_count,
    os.max_attempt_count,
    os.derivations_with_retries,
    ROUND((os.derivations_with_retries::numeric / NULLIF (os.total_derivations, 0)) * 100, 2) AS retry_rate_percent,
    -- Performance metrics
    ROUND(os.avg_evaluation_duration_ms / 1000.0, 2) AS avg_evaluation_duration_seconds,
    ROUND(os.max_evaluation_duration_ms / 1000.0, 2) AS max_evaluation_duration_seconds,
    -- Activity metrics (last 24h)
    os.scheduled_last_hour,
    os.scheduled_last_day,
    os.completed_last_hour,
    os.completed_last_day,
    -- Workload distribution
    os.derivations_with_commits AS commit_based_derivations,
    os.standalone_derivations,
    -- Current snapshot timestamp
    NOW() AS snapshot_timestamp
FROM
    overall_stats os;

COMMENT ON VIEW public.view_derivation_stats IS 'Comprehensive statistics on all derivations including status counts, success rates, and performance metrics';

-- Additional detailed breakdown view
CREATE OR REPLACE VIEW public.view_derivation_status_breakdown AS
WITH total_derivations AS (
    SELECT
        COUNT(*) AS total_count
    FROM
        public.derivations
)
SELECT
    ds.name AS status_name,
    ds.description AS status_description,
    ds.is_terminal,
    ds.is_success,
    COUNT(*) AS total_count,
    COUNT(*) FILTER (WHERE d.derivation_type = 'nixos') AS nixos_count,
    COUNT(*) FILTER (WHERE d.derivation_type = 'package') AS package_count,
    ROUND(AVG(d.attempt_count), 2) AS avg_attempts,
    ROUND(AVG(d.evaluation_duration_ms) FILTER (WHERE d.evaluation_duration_ms IS NOT NULL) / 1000.0, 2) AS avg_duration_seconds,
    MIN(d.scheduled_at) AS oldest_scheduled,
    MAX(d.scheduled_at) AS newest_scheduled,
    COUNT(*) FILTER (WHERE d.scheduled_at >= NOW() - INTERVAL '24 hours') AS count_last_24h,
    ROUND((COUNT(*)::numeric / td.total_count) * 100, 2) AS percentage_of_total
FROM
    public.derivations d
    JOIN public.derivation_statuses ds ON d.status_id = ds.id
    CROSS JOIN total_derivations td
GROUP BY
    ds.id,
    ds.name,
    ds.description,
    ds.is_terminal,
    ds.is_success,
    td.total_count
ORDER BY
    ds.display_order;

COMMENT ON VIEW public.view_derivation_status_breakdown IS 'Detailed breakdown of derivations by status with counts and timing metrics';

