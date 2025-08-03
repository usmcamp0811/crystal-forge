-- Migration 002: Migrate existing nix_packages data to derivations table
-- Step 1: Migrate existing nix_packages data to derivations
INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, package_name, package_pname, package_version, status, scheduled_at, completed_at)
SELECT
    1 AS commit_id, -- Default commit_id - you may need to adjust this based on your data
    'package' AS derivation_type,
    np.name AS derivation_name,
    np.derivation_path,
    np.name AS package_name,
    np.pname AS package_pname,
    np.version AS package_version,
    'complete' AS status, -- Existing packages are already "built"
    np.created_at AS scheduled_at,
    np.created_at AS completed_at -- Assume they completed when they were created
FROM
    nix_packages np;

