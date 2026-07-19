-- Prevent duplicate active CVE scans for the same derivation.
--
-- The background loop and on-demand scan requests both create scan records
-- independently.  Without this constraint, two concurrent paths can both
-- create a pending scan for the same derivation, leading to duplicate work
-- and race conditions in failure handling.
--
-- `ON CONFLICT DO NOTHING` in the insert query then makes the claim atomic:
-- whichever path inserts first wins; the second silently becomes a no-op.
CREATE UNIQUE INDEX IF NOT EXISTS idx_cve_scans_unique_active
ON cve_scans (derivation_id)
WHERE status IN ('pending', 'in_progress');
