-- Create a migration file (e.g., 0030_add_scan_packages_unique_constraint.sql)
ALTER TABLE scan_packages
    ADD CONSTRAINT scan_packages_scan_id_derivation_id_unique UNIQUE (scan_id, derivation_id);

