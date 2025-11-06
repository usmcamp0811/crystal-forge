CREATE OR REPLACE VIEW view_cache_push_queue AS
WITH job_status_by_dest AS (
    SELECT
        derivation_id,
        cache_destination,
        MAX(
            CASE WHEN status IN ('completed', 'in_progress') THEN
                1
            ELSE
                0
            END) AS has_active,
        MAX(
            CASE WHEN status = 'failed'
                AND attempts < 5 THEN
                1
            ELSE
                0
            END) AS has_retryable,
        COUNT(*) AS job_count,
        MAX(completed_at) AS last_completed_at,
        MAX(attempts) AS max_attempts
    FROM
        cache_push_jobs
    GROUP BY
        derivation_id,
        cache_destination
),
newest_nixos_systems AS (
    SELECT DISTINCT
        d.id AS nixos_id,
        d.completed_at AS nixos_completed_at,
        c.commit_timestamp AS commit_timestamp
    FROM
        derivations d
        JOIN commits c ON d.commit_id = c.id
    WHERE
        d.derivation_type = 'nixos'
        AND d.store_path IS NOT NULL
    ORDER BY
        c.commit_timestamp DESC,
        nixos_completed_at DESC
    LIMIT 20
),
prioritized_derivations AS (
    SELECT
        d.id,
        MAX(nns.commit_timestamp) AS newest_nixos_parent
FROM
    derivations d
    LEFT JOIN derivation_dependencies dd ON dd.depends_on_id = d.id
        LEFT JOIN newest_nixos_systems nns ON nns.nixos_id = dd.derivation_id
    GROUP BY
        d.id
)
SELECT
    d.id,
    d.commit_id,
    d.derivation_type,
    d.derivation_name,
    d.pname,
    d.version,
    d.store_path,
    d.completed_at AS build_completed_at,
    d.status_id AS derivation_status_id,
    ds.name AS derivation_status,
    c.git_commit_hash,
    c.commit_timestamp,
    pd.newest_nixos_parent,
    -- Job status fields (NULL if no destination specified in query)
    js.cache_destination,
    js.has_active,
    js.has_retryable,
    js.job_count,
    js.last_completed_at AS last_push_completed_at,
    js.max_attempts AS current_max_attempts,
    -- Computed fields
    CASE WHEN js.derivation_id IS NULL THEN
        'no_job'
    WHEN js.has_active = 1 THEN
        'has_active'
    WHEN js.has_retryable = 1 THEN
        'retryable'
    ELSE
        'complete_or_failed'
    END AS push_status,
    -- Priority score (higher = more urgent)
    CASE WHEN d.derivation_type = 'nixos' THEN
        100
    ELSE
        0
    END + CASE WHEN js.derivation_id IS NULL THEN
        50
    ELSE
        0
    END + CASE WHEN js.has_retryable = 1 THEN
        25
    ELSE
        0
    END AS priority_score
FROM
    derivations d
    JOIN commits c ON c.id = d.commit_id
    JOIN derivation_statuses ds ON ds.id = d.status_id
    LEFT JOIN prioritized_derivations pd ON pd.id = d.id
    LEFT JOIN job_status_by_dest js ON js.derivation_id = d.id
WHERE
    d.store_path IS NOT NULL
ORDER BY
    CASE WHEN d.derivation_type = 'nixos' THEN
        1
    ELSE
        0
    END DESC,
    pd.newest_nixos_parent DESC NULLS LAST,
    c.commit_timestamp DESC,
    d.completed_at ASC NULLS LAST;

