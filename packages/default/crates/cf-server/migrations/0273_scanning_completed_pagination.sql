-- Completed pagination reads terminal rows globally by the authoritative
-- terminal timestamp and immutable scan identity. The partial predicate keeps
-- active lifecycle writes out of this read-optimized index.
CREATE INDEX cve_scans_terminal_page
    ON cve_scans (completed_at DESC, id DESC)
    INCLUDE (
        derivation_id, status, critical_count, high_count, medium_count,
        low_count
    )
    WHERE status IN ('completed', 'failed') AND completed_at IS NOT NULL;

-- Per-system history and revision classification start from one derivation and
-- then apply the same terminal key. This index avoids sorting all terminal
-- history when one exact system configuration is selected.
CREATE INDEX cve_scans_derivation_terminal_page
    ON cve_scans (derivation_id, completed_at DESC, id DESC)
    INCLUDE (status)
    WHERE status IN ('completed', 'failed') AND completed_at IS NOT NULL;

-- Current-revision classification repeatedly reads the newest reported state
-- for a hostname. INCLUDE permits the lifecycle probe to obtain the store path
-- from the index without widening its ordering key.
CREATE INDEX IF NOT EXISTS system_states_scanning_lifecycle
    ON system_states (hostname, timestamp DESC NULLS LAST, id DESC)
    INCLUDE (store_path);
