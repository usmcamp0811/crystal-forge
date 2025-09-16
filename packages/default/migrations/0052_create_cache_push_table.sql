-- Migration: Add cache push status
-- This should be added to your migrations directory
-- Add the new status to derivation_statuses table
INSERT INTO derivation_statuses (name, description, is_terminal, is_success, display_order)
    VALUES ('cache-pushed', 'Derivation has been pushed to cache', TRUE, TRUE, 110);

-- Add cache push tracking table
CREATE TABLE cache_push_jobs (
    id serial PRIMARY KEY,
    derivation_id integer NOT NULL REFERENCES derivations (id) ON DELETE CASCADE,
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'in_progress', 'completed', 'failed')),
    store_path text,
    scheduled_at timestamptz NOT NULL DEFAULT NOW(),
    started_at timestamptz,
    completed_at timestamptz,
    attempts integer NOT NULL DEFAULT 0,
    error_message text,
    push_size_bytes bigint,
    push_duration_ms integer,
    cache_destination text,
    -- Ensure we don't have duplicate pending jobs for the same derivation
    UNIQUE (derivation_id) DEFERRABLE INITIALLY DEFERRED
);

CREATE INDEX idx_cache_push_jobs_status ON cache_push_jobs (status);

CREATE INDEX idx_cache_push_jobs_derivation_id ON cache_push_jobs (derivation_id);

CREATE INDEX idx_cache_push_jobs_scheduled_at ON cache_push_jobs (scheduled_at);

