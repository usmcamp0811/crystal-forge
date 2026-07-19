-- Migration 022: Remove old derivation_path columns and dependencies
-- Drop all dependent objects first, then columns
-- Step 1: Drop views that depend on derivation_path columns and old table references
DROP VIEW IF EXISTS view_critical_vulnerabilities_alert;

DROP VIEW IF EXISTS view_system_vulnerabilities;

-- Step 2: Verify data migration succeeded
-- (Uncomment these to verify before proceeding)
-- SELECT COUNT(*) FROM package_vulnerabilities WHERE derivation_id IS NULL;
-- SELECT COUNT(*) FROM scan_packages WHERE derivation_id IS NULL;
-- Step 3: Add NOT NULL constraints to new foreign keys
ALTER TABLE package_vulnerabilities
    ALTER COLUMN derivation_id SET NOT NULL;

ALTER TABLE scan_packages
    ALTER COLUMN derivation_id SET NOT NULL;

-- Step 4: Drop the unique constraint that includes derivation_path
ALTER TABLE package_vulnerabilities
    DROP CONSTRAINT IF EXISTS package_vulnerabilities_derivation_path_cve_id_key;

-- Step 5: Drop foreign key constraints on derivation_path
ALTER TABLE package_vulnerabilities
    DROP CONSTRAINT IF EXISTS package_vulnerabilities_derivation_path_fkey;

ALTER TABLE scan_packages
    DROP CONSTRAINT IF EXISTS scan_packages_derivation_path_fkey;

-- Step 6: Now we can safely drop the old derivation_path columns
ALTER TABLE package_vulnerabilities
    DROP COLUMN derivation_path;

ALTER TABLE scan_packages
    DROP COLUMN derivation_path;

-- Step 7: Add the new unique constraint using derivation_id
ALTER TABLE package_vulnerabilities
    ADD CONSTRAINT package_vulnerabilities_derivation_id_cve_id_key UNIQUE (derivation_id, cve_id);

-- Step 8: Clean up old indexes
DROP INDEX IF EXISTS idx_evaluation_targets_status;

DROP INDEX IF EXISTS idx_cve_scans_evaluation_target;

DROP INDEX IF EXISTS idx_package_vulnerabilities_derivation;

DROP INDEX IF EXISTS idx_scan_packages_derivation;

