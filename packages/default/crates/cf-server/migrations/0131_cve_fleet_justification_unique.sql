-- Fix fleet-wide CVE justification upsert correctness.
--
-- The system_cve_justifications table has a composite PRIMARY KEY
-- (system_id, cve_id).  In PostgreSQL, NULL values are never considered
-- equal under a normal unique constraint, so repeated fleet-wide saves
-- (system_id IS NULL) insert duplicate rows instead of updating the
-- existing one.
--
-- This partial unique index makes fleet-wide rows (system_id IS NULL)
-- conflict-detectable so that INSERT … ON CONFLICT (cve_id) WHERE
-- system_id IS NULL DO UPDATE can perform a true upsert.

CREATE UNIQUE INDEX IF NOT EXISTS idx_system_cve_justifications_fleet_unique
    ON system_cve_justifications (cve_id)
    WHERE system_id IS NULL;

COMMENT ON INDEX idx_system_cve_justifications_fleet_unique IS
    'Ensures at most one fleet-wide (system_id IS NULL) justification per CVE, '
    'enabling correct ON CONFLICT upsert semantics for fleet-wide triage actions.';
