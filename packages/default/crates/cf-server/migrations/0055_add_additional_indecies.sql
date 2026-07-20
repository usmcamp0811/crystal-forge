-- For cache push queries
CREATE INDEX IF NOT EXISTS idx_cache_push_jobs_derivation_status ON cache_push_jobs (derivation_id, status);

-- For CVE scan queries
CREATE INDEX IF NOT EXISTS idx_cve_scans_derivation_status ON cve_scans (derivation_id, status);

-- For derivation status lookups
CREATE INDEX IF NOT EXISTS idx_derivations_status_path ON derivations (status_id, derivation_path)
WHERE
    derivation_path IS NOT NULL;

-- For derivation type + status queries
CREATE INDEX IF NOT EXISTS idx_derivations_type_status ON derivations (derivation_type, status_id);

-- For nested EXISTS queries
CREATE INDEX IF NOT EXISTS idx_cache_push_jobs_derivation_failed ON cache_push_jobs (derivation_id, status, attempts)
WHERE
    status = 'failed';

-- For CVE scan lookups
CREATE INDEX IF NOT EXISTS idx_cve_scans_derivation_pending ON cve_scans (derivation_id, status, scheduled_at)
WHERE
    status IN ('pending', 'in_progress');

-- Remove CONCURRENTLY from these two:
CREATE INDEX IF NOT EXISTS idx_cache_push_jobs_derivation_status_attempts ON cache_push_jobs (derivation_id, status, attempts);

CREATE INDEX IF NOT EXISTS idx_d_ready_cve ON derivations (completed_at, id)
WHERE
    derivation_path IS NOT NULL AND status_id IN (10, 11);

