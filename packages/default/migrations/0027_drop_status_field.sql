-- Migration: Drop status column and update all dependencies to use status_id
-- Step 1: Drop views that reference the status column
-- We'll recreate them using status_id/derivations_with_both_status
DROP VIEW IF EXISTS view_commit_deployment_timeline;

DROP VIEW IF EXISTS view_critical_vulnerabilities_alert;

DROP VIEW IF EXISTS view_environment_security_posture;

DROP VIEW IF EXISTS view_evaluation_pipeline_debug;

DROP VIEW IF EXISTS view_evaluation_queue_status;

DROP VIEW IF EXISTS view_recent_failed_evaluations;

DROP VIEW IF EXISTS view_systems_latest_flake_commit;

DROP VIEW IF EXISTS view_system_vulnerabilities;

DROP VIEW IF EXISTS view_systems_current_state;

DROP VIEW IF EXISTS view_systems_cve_summary;

DROP VIEW IF EXISTS view_systems_status_table;

-- Step 2: Update any functions that might reference status
-- Recreate get_latest_cve_scan function to use derivations_with_both_status
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
        derivations_with_both_status d
        JOIN cve_scans cs ON d.id = cs.derivation_id
    WHERE
        d.derivation_name = derivation_name
        AND d.derivation_type = 'nixos'
        AND d.status_name = 'complete'
        AND cs.completed_at IS NOT NULL
    ORDER BY
        cs.completed_at DESC
    LIMIT 1;
END;
$$;

-- Step 3: Drop the old status column and constraints
ALTER TABLE derivations
    DROP CONSTRAINT IF EXISTS valid_status;

ALTER TABLE derivations
    DROP COLUMN status;

-- Step 4: Create the final clean view without the old status column
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

-- Step 5: Drop the transition view
DROP VIEW derivations_with_both_status;

