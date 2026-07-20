-- Migration 003: Update tables that referenced evaluation_targets/nix_packages
-- Step 1: Update cve_scans table reference
ALTER TABLE cve_scans RENAME COLUMN evaluation_target_id TO derivation_id;

ALTER TABLE cve_scans
    DROP CONSTRAINT cve_scans_evaluation_target_id_fkey;

ALTER TABLE cve_scans
    ADD CONSTRAINT cve_scans_derivation_id_fkey FOREIGN KEY (derivation_id) REFERENCES derivations (id) ON DELETE CASCADE;

-- Step 2: Update package_vulnerabilities to reference derivations
ALTER TABLE package_vulnerabilities
    ADD COLUMN derivation_id integer REFERENCES derivations (id) ON DELETE CASCADE;

-- Populate derivation_id from derivation_path
UPDATE
    package_vulnerabilities pv
SET
    derivation_id = d.id
FROM
    derivations d
WHERE
    pv.derivation_path = d.derivation_path
    AND d.derivation_type = 'package';

-- Step 3: Update scan_packages to reference derivations
ALTER TABLE scan_packages
    ADD COLUMN derivation_id integer REFERENCES derivations (id) ON DELETE CASCADE;

-- Populate derivation_id from derivation_path
UPDATE
    scan_packages sp
SET
    derivation_id = d.id
FROM
    derivations d
WHERE
    sp.derivation_path = d.derivation_path
    AND d.derivation_type = 'package';

-- Step 4: Add indexes for the new foreign keys
CREATE INDEX idx_cve_scans_derivation_id ON cve_scans (derivation_id);

CREATE INDEX idx_package_vulnerabilities_derivation_id ON package_vulnerabilities (derivation_id);

CREATE INDEX idx_scan_packages_derivation_id ON scan_packages (derivation_id);

