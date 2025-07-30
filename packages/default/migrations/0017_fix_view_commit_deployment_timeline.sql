-- Debug and fix view_commit_deployment_timeline
-- ============================================================================

-- First, let's see what status values actually exist in evaluation_targets
SELECT DISTINCT status, COUNT(*) 
FROM evaluation_targets 
GROUP BY status 
ORDER BY status;

-- Check if there are any evaluation_targets with derivation_path (completed builds)
SELECT COUNT(*) as total_evaluations,
       COUNT(*) FILTER (WHERE derivation_path IS NOT NULL) as with_derivation_path,
       COUNT(*) FILTER (WHERE status LIKE '%complete%') as completed_status
FROM evaluation_targets;

-- Check recent commits and their evaluations
SELECT 
    c.id as commit_id,
    c.git_commit_hash,
    LEFT(c.git_commit_hash, 8) as short_hash,
    c.commit_timestamp,
    f.name as flake_name,
    et.target_name,
    et.status,
    et.derivation_path,
    et.completed_at
FROM commits c
JOIN flakes f ON c.flake_id = f.id
LEFT JOIN evaluation_targets et ON c.id = et.commit_id
WHERE c.commit_timestamp >= NOW() - INTERVAL '30 days'
ORDER BY c.commit_timestamp DESC
LIMIT 20;

-- Check if there are any system_states that match evaluation derivation paths
SELECT 
    et.derivation_path as eval_derivation,
    COUNT(DISTINCT ss.hostname) as matching_systems,
    STRING_AGG(DISTINCT ss.hostname, ', ') as hostnames
FROM evaluation_targets et
LEFT JOIN system_states ss ON et.derivation_path = ss.derivation_path
WHERE et.derivation_path IS NOT NULL
GROUP BY et.derivation_path
LIMIT 10;

-- Now let's create a more flexible version of the view that shows what's actually happening
DROP VIEW IF EXISTS view_commit_deployment_timeline;

CREATE OR REPLACE VIEW view_commit_deployment_timeline AS
SELECT
    c.flake_id,
    f.name AS flake_name,
    c.id AS commit_id,
    c.git_commit_hash,
    LEFT(c.git_commit_hash, 8) AS short_hash,
    c.commit_timestamp,
    -- Show evaluation info
    COUNT(DISTINCT et.id) as total_evaluations,
    COUNT(DISTINCT et.id) FILTER (WHERE et.derivation_path IS NOT NULL) as successful_evaluations,
    STRING_AGG(DISTINCT et.status, ', ') as evaluation_statuses,
    STRING_AGG(DISTINCT et.target_name, ', ') as evaluated_targets,
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
    commits c
    JOIN flakes f ON c.flake_id = f.id
    LEFT JOIN evaluation_targets et ON c.id = et.commit_id
    LEFT JOIN system_states ss ON et.derivation_path = ss.derivation_path 
        AND et.derivation_path IS NOT NULL  -- Only join for successful evaluations
    LEFT JOIN ( 
        SELECT DISTINCT ON (hostname)
            hostname,
            derivation_path
        FROM
            system_states
        ORDER BY
            hostname,
            timestamp DESC
    ) current_sys ON et.derivation_path = current_sys.derivation_path
        AND et.target_name = current_sys.hostname
        AND et.derivation_path IS NOT NULL  -- Only for successful evaluations
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

COMMENT ON VIEW view_commit_deployment_timeline IS 'Shows commit deployment timeline with evaluation and deployment status. Now includes evaluation info even for non-deployed commits.';

-- Create a simpler view to see what's actually in the pipeline
CREATE OR REPLACE VIEW view_evaluation_pipeline_debug AS
SELECT
    f.name as flake_name,
    c.git_commit_hash,
    LEFT(c.git_commit_hash, 8) as short_hash,
    c.commit_timestamp,
    et.target_name,
    et.status,
    et.scheduled_at,
    et.started_at,
    et.completed_at,
    CASE WHEN et.derivation_path IS NOT NULL THEN 'HAS_DERIVATION' ELSE 'NO_DERIVATION' END as has_derivation,
    et.attempt_count,
    et.error_message
FROM commits c
JOIN flakes f ON c.flake_id = f.id
LEFT JOIN evaluation_targets et ON c.id = et.commit_id
WHERE c.commit_timestamp >= NOW() - INTERVAL '7 days'
ORDER BY c.commit_timestamp DESC, et.target_name;

COMMENT ON VIEW view_evaluation_pipeline_debug IS 'Debug view to see what is actually happening in the evaluation pipeline';
