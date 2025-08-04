-- Migration 005: Drop nix_packages table and update views/functions
-- Only run this after verifying all data migration is complete
-- Step 1: Drop the old nix_packages table
DROP TABLE nix_packages;

-- Step 2: Update functions that referenced evaluation_targets
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
        JOIN cve_scans cs ON d.id = cs.derivation_id
    WHERE
        d.derivation_name = derivation_name
        AND d.derivation_type = 'nixos'
        AND d.status = 'complete'
        AND cs.completed_at IS NOT NULL
    ORDER BY
        cs.completed_at DESC
    LIMIT 1;
END;
$$;

-- Step 3: Create view to replace nix_packages for backward compatibility
CREATE VIEW nix_packages AS
SELECT
    derivation_path,
    package_name AS name,
    package_pname AS pname,
    package_version AS version,
    scheduled_at AS created_at
FROM
    derivations
WHERE
    derivation_type = 'package';

-- Step 4: Create view to replace evaluation_targets for backward compatibility
CREATE VIEW evaluation_targets AS
SELECT
    id,
    commit_id,
    derivation_type AS target_type,
    derivation_name AS target_name,
    derivation_path,
    scheduled_at,
    completed_at,
    attempt_count,
    status,
    started_at,
    evaluation_duration_ms,
    error_message
FROM
    derivations;

