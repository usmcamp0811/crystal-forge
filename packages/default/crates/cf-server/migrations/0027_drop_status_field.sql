-- Aggressive cleanup: Drop all views that could reference derivations.status
-- Step 1: Drop ALL views that reference derivations table status
DROP VIEW IF EXISTS view_commit_deployment_timeline CASCADE;

DROP VIEW IF EXISTS view_current_builds CASCADE;

DROP VIEW IF EXISTS view_current_commits CASCADE;

DROP VIEW IF EXISTS view_current_dry_runs CASCADE;

DROP VIEW IF EXISTS view_current_scans CASCADE;

DROP VIEW IF EXISTS view_environment_security_posture CASCADE;

DROP VIEW IF EXISTS view_evaluation_pipeline_debug CASCADE;

DROP VIEW IF EXISTS view_evaluation_queue_status CASCADE;

DROP VIEW IF EXISTS view_failed_build_commits CASCADE;

DROP VIEW IF EXISTS view_failed_cve_scan_status CASCADE;

DROP VIEW IF EXISTS view_failed_dry_run_commits CASCADE;

DROP VIEW IF EXISTS view_failed_evaluations CASCADE;

DROP VIEW IF EXISTS view_recent_failed_evaluations CASCADE;

DROP VIEW IF EXISTS view_systems_latest_flake_commit CASCADE;

DROP VIEW IF EXISTS view_systems_current_state CASCADE;

DROP VIEW IF EXISTS view_systems_cve_summary CASCADE;

DROP VIEW IF EXISTS view_systems_status_table CASCADE;

-- Step 2: Drop the transition view
DROP VIEW IF EXISTS derivations_with_both_status CASCADE;

-- Step 3: Update the function to use status_id
DROP FUNCTION IF EXISTS get_latest_cve_scan (text);

CREATE FUNCTION public.get_latest_cve_scan (derivation_name text)
    RETURNS TABLE (
        scan_id uuid,
        completed_at timestamp with time zone,
        total_vulnerabilities integer,
        critical_count integer,
        high_count integer)
    LANGUAGE plpgsql
    AS $$
BEGIN
    RETURN QUERY
    SELECT
        cs.id,
        cs.completed_at,
        cs.total_vulnerabilities,
        cs.critical_count,
        cs.high_count
    FROM
        derivations d
        JOIN derivation_statuses ds ON d.status_id = ds.id
        JOIN cve_scans cs ON d.id = cs.derivation_id
    WHERE
        d.derivation_name = derivation_name
        AND d.derivation_type = 'nixos'
        AND ds.name = 'complete'
        AND cs.completed_at IS NOT NULL
    ORDER BY
        cs.completed_at DESC
    LIMIT 1;
END;
$$;

-- Step 4: Drop the CHECK constraint
ALTER TABLE derivations
    DROP CONSTRAINT IF EXISTS valid_status;

-- Step 5: Drop the status column
ALTER TABLE derivations
    DROP COLUMN status;

-- Step 6: Create the final clean status view
CREATE VIEW derivations_with_status AS
SELECT
    d.*,
    ds.name AS status_name,
    ds.description AS status_description,
    ds.is_terminal,
    ds.is_success,
    ds.display_order
FROM
    derivations d
    JOIN derivation_statuses ds ON d.status_id = ds.id;

