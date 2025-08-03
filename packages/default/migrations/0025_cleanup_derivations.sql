-- Migration: Clean up derivations table structure
-- 1. Remove redundant package_name column (keep derivation_name)
-- 2. Rename package_pname -> pname, package_version -> version
-- 3. Populate pname for nixos derivations
-- Step 1: For nixos derivations, extract pname from derivation_name
-- Assuming nixos derivation names are like "my-server" or hostnames
UPDATE
    derivations
SET
    package_pname = derivation_name
WHERE
    derivation_type = 'nixos'
    AND package_pname IS NULL;

-- Step 2: Rename columns
ALTER TABLE derivations RENAME COLUMN package_pname TO pname;

ALTER TABLE derivations RENAME COLUMN package_version TO version;

-- Step 3: Drop the redundant package_name column
ALTER TABLE derivations
    DROP COLUMN package_name;

-- Step 4: Update indexes to reflect new column names
DROP INDEX IF EXISTS idx_derivations_package_pname_version;

CREATE INDEX idx_derivations_pname_version ON derivations (pname, version);

