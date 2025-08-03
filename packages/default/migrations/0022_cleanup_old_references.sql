-- Migration 004: Remove old derivation_path columns and add NOT NULL constraints
-- Only run this after verifying the previous migrations worked correctly
-- Step 1: Verify data migration succeeded (run these queries first to check)
-- SELECT COUNT(*) FROM package_vulnerabilities WHERE derivation_id IS NULL;
-- SELECT COUNT(*) FROM scan_packages WHERE derivation_id IS NULL;
-- Step 2: Add NOT NULL constraints to new foreign keys
ALTER TABLE package_vulnerabilities
    ALTER COLUMN derivation_id SET NOT NULL;

ALTER TABLE scan_packages
    ALTER COLUMN derivation_id SET NOT NULL;

-- Step 3: Drop old derivation_path columns from junction tables
ALTER TABLE package_vulnerabilities
    DROP COLUMN derivation_path;

ALTER TABLE scan_packages
    DROP COLUMN derivation_path;

-- Step 4: Update index names to reflect new table name
DROP INDEX IF EXISTS idx_evaluation_targets_status;

DROP INDEX IF EXISTS idx_cve_scans_evaluation_target;

ALTER INDEX idx_cve_scans_derivation_id RENAME TO idx_cve_scans_derivation_id;

