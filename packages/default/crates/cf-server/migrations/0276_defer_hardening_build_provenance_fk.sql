-- A derivation deletion cascades to both its build jobs and hardening scans.
-- Defer the provenance check until the transaction ends so those sibling
-- cascades can complete in either internal order. A direct build-job deletion
-- still fails at commit while a surviving hardening scan references the job.
ALTER TABLE hardening_scans
    DROP CONSTRAINT hardening_scans_source_build_job_id_fkey,
    ADD CONSTRAINT hardening_scans_source_build_job_id_fkey
        FOREIGN KEY (source_build_job_id)
        REFERENCES build_jobs(id)
        ON DELETE NO ACTION
        DEFERRABLE INITIALLY DEFERRED;
