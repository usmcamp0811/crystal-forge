-- Hotfix: allow the cancel lifecycle introduced in TASK-238.
--
-- The application now writes `cancelling` and `cancelled` to build_jobs.status,
-- but the original CHECK constraint from migration 0083 still only permits
-- ('queued', 'building', 'success', 'failed').
--
-- Without this migration, clicking Stop on a running build fails at the DB
-- layer when the server attempts to update status -> 'cancelling'.

ALTER TABLE build_jobs
    DROP CONSTRAINT IF EXISTS build_jobs_status_check;

ALTER TABLE build_jobs
    ADD CONSTRAINT build_jobs_status_check
    CHECK (
        status IN (
            'queued',
            'building',
            'cancelling',
            'cancelled',
            'success',
            'failed'
        )
    );
