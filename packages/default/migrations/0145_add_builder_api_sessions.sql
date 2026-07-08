-- Migration 0145: Track API builder process sessions
--
-- A builder UUID is a persistent identity tied to the registered key. Runtime
-- recovery must distinguish that identity from a single running builder process
-- so overlapping restarts do not requeue work that is still actively building.

ALTER TABLE builders
    ADD COLUMN IF NOT EXISTS current_session_id UUID,
    ADD COLUMN IF NOT EXISTS current_session_started_at TIMESTAMPTZ;

ALTER TABLE build_jobs
    ADD COLUMN IF NOT EXISTS builder_session_id UUID;

CREATE INDEX IF NOT EXISTS idx_build_jobs_builder_session_active
    ON build_jobs(builder_id, builder_session_id)
    WHERE status = 'building';

COMMENT ON COLUMN builders.current_session_id IS 'Current API builder process/session UUID. Distinct from persistent builder identity.';
COMMENT ON COLUMN builders.current_session_started_at IS 'Time the current API builder process/session was established.';
COMMENT ON COLUMN build_jobs.builder_session_id IS 'API builder process/session UUID that claimed this job. NULL for legacy claims.';
