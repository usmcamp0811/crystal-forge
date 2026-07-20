CREATE OR REPLACE VIEW public.view_commit_deployment_timeline AS
WITH commit_evaluations AS (
    -- Get evaluation status for each commit
    SELECT
        c.flake_id,
        c.id AS commit_id,
        c.git_commit_hash,
        c.commit_timestamp,
        COUNT(DISTINCT d.id) AS total_evaluations,
        COUNT(DISTINCT d.id) FILTER (WHERE ds.is_success = TRUE) AS successful_evaluations,
        STRING_AGG(DISTINCT ds.name, ', ') AS evaluation_statuses,
        STRING_AGG(DISTINCT d.derivation_name, ', ') AS evaluated_targets
    FROM
        commits c
        LEFT JOIN derivations d ON c.id = d.commit_id
            AND d.derivation_type = 'nixos'
        LEFT JOIN derivation_statuses ds ON d.status_id = ds.id
    WHERE
        c.commit_timestamp >= NOW() - INTERVAL '30 days'
    GROUP BY
        c.flake_id,
        c.id,
        c.git_commit_hash,
        c.commit_timestamp
),
latest_successful_by_system AS (
    -- For each system, find the most recent successful derivation
    SELECT DISTINCT ON (d.derivation_name)
        d.derivation_name,
        d.commit_id,
        c.git_commit_hash,
        c.commit_timestamp AS derivation_commit_time
    FROM
        derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        JOIN commits c ON d.commit_id = c.id
    WHERE
        d.derivation_type = 'nixos'
        AND ds.is_success = TRUE
        AND c.commit_timestamp >= NOW() - INTERVAL '30 days'
    ORDER BY
        d.derivation_name,
        c.commit_timestamp DESC
),
system_first_seen_with_commit AS (
    -- Find when we first saw each system after each commit was made
    -- This approximates "deployment time"
    SELECT
        lsbs.commit_id,
        lsbs.derivation_name,
        MIN(ss.timestamp) FILTER (WHERE ss.timestamp > lsbs.derivation_commit_time) AS first_seen_after_commit
    FROM
        latest_successful_by_system lsbs
        JOIN system_states ss ON ss.hostname = lsbs.derivation_name
    GROUP BY
        lsbs.commit_id,
        lsbs.derivation_name
),
current_deployments AS (
    -- Get current deployment status
    SELECT DISTINCT ON (hostname)
        hostname,
        timestamp AS current_timestamp
    FROM
        system_states
    ORDER BY
        hostname,
        timestamp DESC
),
commit_deployment_stats AS (
    -- Aggregate deployment info per commit
    SELECT
        lsbs.commit_id,
        COUNT(DISTINCT lsbs.derivation_name) AS systems_with_successful_evaluation,
        COUNT(DISTINCT sfswc.derivation_name) AS systems_seen_after_commit,
        MIN(sfswc.first_seen_after_commit) AS first_system_seen,
        MAX(sfswc.first_seen_after_commit) AS last_system_seen,
        STRING_AGG(DISTINCT sfswc.derivation_name, ', ') AS systems_seen_list,
        -- Current systems running this commit (based on latest successful derivation)
        COUNT(DISTINCT CASE WHEN cd.hostname IS NOT NULL THEN
                lsbs.derivation_name
            END) AS currently_active_systems,
        STRING_AGG(DISTINCT CASE WHEN cd.hostname IS NOT NULL THEN
                lsbs.derivation_name
            END, ', ') AS currently_active_systems_list
    FROM
        latest_successful_by_system lsbs
        LEFT JOIN system_first_seen_with_commit sfswc ON lsbs.commit_id = sfswc.commit_id
            AND lsbs.derivation_name = sfswc.derivation_name
        LEFT JOIN current_deployments cd ON lsbs.derivation_name = cd.hostname
    GROUP BY
        lsbs.commit_id
)
SELECT
    ce.flake_id,
    f.name AS flake_name,
    ce.commit_id,
    ce.git_commit_hash,
    LEFT (ce.git_commit_hash,
        8) AS short_hash,
    ce.commit_timestamp,
    ce.total_evaluations,
    ce.successful_evaluations,
    ce.evaluation_statuses,
    ce.evaluated_targets,
    -- "Deployment" timeline (really first-seen timeline)
    cds.first_system_seen AS first_deployment,
    cds.last_system_seen AS last_deployment,
    COALESCE(cds.systems_seen_after_commit, 0) AS total_systems_deployed,
    COALESCE(cds.currently_active_systems, 0) AS currently_deployed_systems,
    cds.systems_seen_list AS deployed_systems,
    cds.currently_active_systems_list AS currently_deployed_systems_list
FROM
    commit_evaluations ce
    JOIN flakes f ON ce.flake_id = f.id
    LEFT JOIN commit_deployment_stats cds ON ce.commit_id = cds.commit_id
ORDER BY
    ce.commit_timestamp DESC;

COMMENT ON VIEW public.view_commit_deployment_timeline IS 'Shows commit evaluation timeline and approximates deployment by tracking when systems were first seen after commit timestamp.';

