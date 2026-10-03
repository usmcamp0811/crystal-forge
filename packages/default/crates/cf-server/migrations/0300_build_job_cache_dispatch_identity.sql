-- Retain dispatch identity after destination deletion; completion must reject
-- a deleted destination instead of silently losing the original selection.
ALTER TABLE build_jobs
    ADD COLUMN dispatched_cache_destination_id INTEGER,
    ADD COLUMN cache_dispatch_recorded_at TIMESTAMPTZ;

COMMENT ON COLUMN build_jobs.dispatched_cache_destination_id IS
    'Cache destination selected for the current builder/session dispatch. NULL means legacy or disabled publication; see cache_dispatch_recorded_at.';
COMMENT ON COLUMN build_jobs.cache_dispatch_recorded_at IS
    'Records immutable cache selection for the current claim, including disabled publication. Reset together with destination identity on every new claim.';
