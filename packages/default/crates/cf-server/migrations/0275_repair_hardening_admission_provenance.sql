-- Preserve truthful provenance for rows that existed before durable Hardening
-- admission metadata. Migration 0274 and this migration ship in the same
-- release, so no application process can insert a new manual row between them.
-- Existing rows can include both manual requests and the removed
-- evaluation-time automatic path. Their original trigger cannot be proved.
UPDATE hardening_scans
SET source_trigger = 'legacy'
WHERE source_trigger = 'manual' AND source_build_job_id IS NULL;

ALTER TABLE hardening_scans
    DROP CONSTRAINT hardening_scan_source_trigger_check,
    ADD CONSTRAINT hardening_scan_source_trigger_check
        CHECK (source_trigger IN ('manual', 'legacy', 'post_build', 'backfill')),
    DROP CONSTRAINT hardening_scan_manual_has_no_build_provenance,
    ADD CONSTRAINT hardening_scan_manual_has_no_build_provenance
        CHECK (source_trigger NOT IN ('manual', 'legacy') OR source_build_job_id IS NULL),
    DROP CONSTRAINT hardening_scans_source_build_job_id_fkey,
    ADD CONSTRAINT hardening_scans_source_build_job_id_fkey
        FOREIGN KEY (source_build_job_id) REFERENCES build_jobs(id) ON DELETE RESTRICT;

-- IDEMPOTENCY: Build-job retention must preserve the source row while an
-- automatic Hardening event references it. Keeping the foreign key and unique
-- index intact preserves both audit provenance and the permanent per-build
-- admission slot.
