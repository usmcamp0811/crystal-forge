-- Migration: Clean up derivations table structure
-- Drop dependent views first, then modify columns
-- Step 1: Drop views that likely reference package_name
-- These will be recreated in the view update migrations with correct column references
DROP VIEW IF EXISTS view_system_vulnerabilities;

DROP VIEW IF EXISTS view_critical_vulnerabilities_alert;

-- Step 2: For nixos derivations, extract pname from derivation_name
-- Assuming nixos derivation names are like "my-server" or hostnames
UPDATE
    derivations
SET
    package_pname = derivation_name
WHERE
    derivation_type = 'nixos'
    AND package_pname IS NULL;

-- Step 3: Rename columns
ALTER TABLE derivations RENAME COLUMN package_pname TO pname;

ALTER TABLE derivations RENAME COLUMN package_version TO version;

-- Step 4: Drop the redundant package_name column
ALTER TABLE derivations
    DROP COLUMN package_name;

-- Step 5: Update indexes to reflect new column names
DROP INDEX IF EXISTS idx_derivations_package_pname_version;

CREATE INDEX idx_derivations_pname_version ON derivations (pname, version);

