-- For CVE scan lookups
CREATE INDEX IF NOT EXISTS idx_cve_scans_derivation_pending ON cve_scans (derivation_id, status, scheduled_at)
WHERE
    status IN ('pending', 'in_progress');

