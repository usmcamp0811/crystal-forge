-- TASK-440: durable Config Inspector execution ownership.
--
-- Migration 0252 predates a Config Inspector worker. A running row from that
-- schema cannot represent a live execution, so normalize it to queued work
-- before adding ownership constraints. Terminal history is preserved.

ALTER TABLE config_inspection_jobs
    ADD COLUMN execution_id uuid,
    ADD COLUMN execution_heartbeat_at timestamptz;

UPDATE config_inspection_jobs
SET status = 'queued',
    started_at = NULL,
    completed_at = NULL,
    error = NULL,
    updated_at = now()
WHERE status = 'running';

ALTER TABLE config_inspection_jobs
    ADD CONSTRAINT config_inspection_jobs_execution_pair_ck
        CHECK (
            (execution_id IS NULL AND execution_heartbeat_at IS NULL)
            OR (execution_id IS NOT NULL AND execution_heartbeat_at IS NOT NULL)
        ),
    ADD CONSTRAINT config_inspection_jobs_queued_execution_ck
        CHECK (
            status <> 'queued'
            OR (execution_id IS NULL AND execution_heartbeat_at IS NULL)
        ),
    ADD CONSTRAINT config_inspection_jobs_running_execution_ck
        CHECK (
            status <> 'running'
            OR (execution_id IS NOT NULL AND execution_heartbeat_at IS NOT NULL)
        );

CREATE UNIQUE INDEX config_inspection_jobs_execution_id_idx
    ON config_inspection_jobs (execution_id)
    WHERE execution_id IS NOT NULL;

CREATE INDEX config_inspection_jobs_stale_recovery_idx
    ON config_inspection_jobs (execution_heartbeat_at, id)
    WHERE status = 'running';
