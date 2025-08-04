CREATE OR REPLACE VIEW public.view_commit_deployment_timeline AS
SELECT
    c.flake_id,
    f.name AS flake_name,
    c.id AS commit_id,
    c.git_commit_hash,
    LEFT (c.git_commit_hash,
        8) AS short_hash,
    c.commit_timestamp,
    -- Show evaluation info
    COUNT(DISTINCT d.id) AS total_evaluations,
    COUNT(DISTINCT d.id) FILTER (WHERE d.derivation_path IS NOT NULL) AS successful_evaluations,
    STRING_AGG(DISTINCT ds.name, ', ') AS evaluation_statuses,
    STRING_AGG(DISTINCT d.derivation_name, ', ') AS evaluated_targets,
    -- Deployment info (only for successful evaluations with derivation_path)
    MIN(ss.timestamp) AS first_deployment,
    MAX(ss.timestamp) AS last_deployment,
    COUNT(DISTINCT ss.hostname) AS total_systems_deployed,
    COUNT(DISTINCT current_sys.hostname) AS currently_deployed_systems,
    STRING_AGG(DISTINCT ss.hostname, ', ') AS deployed_systems,
    STRING_AGG(DISTINCT CASE WHEN current_sys.hostname IS NOT NULL THEN
            current_sys.hostname
        END, ', ') AS currently_deployed_systems_list
FROM
    public.commits c
    JOIN public.flakes f ON c.flake_id = f.id
    LEFT JOIN public.derivations d ON c.id = d.commit_id
        AND d.derivation_type = 'nixos'
    LEFT JOIN public.derivation_statuses ds ON d.status_id = ds.id
        AND ds.name IN ('build-failed', 'build-complete', 'dry-run-complete', 'build-pending', 'build-in-progress') -- Only relevant statuses
    LEFT JOIN public.system_states ss ON d.derivation_path = ss.derivation_path
        AND d.derivation_path IS NOT NULL -- Only join for successful evaluations
    LEFT JOIN (
        -- Get current deployment state per hostname
        SELECT DISTINCT ON (hostname)
            hostname,
            derivation_path
        FROM
            public.system_states
        ORDER BY
            hostname,
            timestamp DESC) current_sys ON d.derivation_path = current_sys.derivation_path
    AND d.derivation_name = current_sys.hostname
    AND d.derivation_path IS NOT NULL -- Only for successful evaluations
WHERE
    c.commit_timestamp >= NOW() - INTERVAL '30 days'
GROUP BY
    c.flake_id,
    c.id,
    c.git_commit_hash,
    c.commit_timestamp,
    f.name
ORDER BY
    c.commit_timestamp DESC;

COMMENT ON VIEW public.view_commit_deployment_timeline IS 'Shows commit deployment timeline for successfully evaluated commits that have been deployed to systems.';

