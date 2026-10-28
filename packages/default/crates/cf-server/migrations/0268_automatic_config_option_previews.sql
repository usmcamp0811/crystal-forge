-- TASK-440: distinguish bounded automatic option-value previews from
-- explicit inspection so explicit work is never queued behind them.
--
-- Automatic previews are speculative, per-viewport, and use a short
-- server-enforced execution budget applied in application code. Only an
-- exact option request can ever be automatic; every other scoped operation
-- (root, prefix, provenance, the configured index) remains explicit-only.
-- Reuse of an existing automatic-owned queued request by an explicit caller
-- promotes it in place so the shared row is reserved ahead of any remaining
-- automatic work at the same priority tier.

ALTER TABLE config_observation_requests
    ADD COLUMN is_automatic boolean NOT NULL DEFAULT false;

ALTER TABLE config_observation_requests
    ADD CONSTRAINT config_observation_requests_automatic_kind_ck
        CHECK (NOT is_automatic OR kind = 'option');

COMMENT ON COLUMN config_observation_requests.is_automatic IS
    'True only for a bounded automatic value preview started without an explicit click. A reused row is promoted to false the moment explicit interest joins it.';

DROP INDEX config_observation_requests_queue_idx;
CREATE INDEX config_observation_requests_queue_idx
    ON config_observation_requests (priority, is_automatic, scheduled_at, created_at, id)
    WHERE status IN ('queued', 'waiting_for_capacity');
