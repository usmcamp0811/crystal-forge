-- Allow fleet-wide CVE justifications (system_id IS NULL) while preserving
-- per-system uniqueness for concrete system rows.
--
-- Historical schema used PRIMARY KEY (system_id, cve_id), which implicitly
-- enforced NOT NULL on system_id and blocked fleet-wide rows.
--
-- New shape:
--   - system_id nullable
--   - per-system uniqueness via partial unique index
--   - fleet-wide uniqueness via idx_system_cve_justifications_fleet_unique (0131)

ALTER TABLE public.system_cve_justifications
    DROP CONSTRAINT IF EXISTS system_cve_justifications_pkey;

ALTER TABLE public.system_cve_justifications
    ALTER COLUMN system_id DROP NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_system_cve_justifications_system_cve_unique
    ON public.system_cve_justifications (system_id, cve_id)
    WHERE system_id IS NOT NULL;

COMMENT ON INDEX idx_system_cve_justifications_system_cve_unique IS
    'Ensures one justification per (system_id, cve_id) for system-scoped triage records.';
