-- Migration 020: Fix constraints and migrate nix_packages data to derivations
-- Step 1: Drop the problematic unique constraint that doesn't make sense anymore
ALTER TABLE derivations
    DROP CONSTRAINT IF EXISTS tbl_evaluation_targets_commit_id_target_type_target_name_key;

-- Step 2: Add a more appropriate constraint - derivation_path should be unique
ALTER TABLE derivations
    ADD CONSTRAINT derivations_derivation_path_unique UNIQUE (derivation_path);

-- Step 3: Migrate existing nix_packages data to derivations
INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, package_name, package_pname, package_version, status, scheduled_at, completed_at)
SELECT
    1 AS commit_id, -- Placeholder for migrated packages
    'package' AS derivation_type,
    np.name AS derivation_name,
    np.derivation_path,
    np.name AS package_name,
    np.pname AS package_pname,
    np.version AS package_version,
    'complete' AS status,
    np.created_at AS scheduled_at,
    np.created_at AS completed_at
FROM
    nix_packages np
ON CONFLICT (derivation_path)
    DO NOTHING;

