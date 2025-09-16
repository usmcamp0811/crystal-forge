-- Add the new status to derivation_statuses table
INSERT INTO derivation_statuses (id, name, description, is_terminal, is_success, display_order)
SELECT
    14,
    'cache-pushed',
    'Derivation has been pushed to cache',
    TRUE,
    TRUE,
    100
WHERE
    NOT EXISTS (
        SELECT
            1
        FROM
            derivation_statuses
        WHERE
            name = 'cache-pushed');

-- Add cache push tracking table
CREATE TABLE IF NOT EXISTS cache_push_jobs (
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
    cache_destination text
);

-- Add indexes
CREATE INDEX IF NOT EXISTS idx_cache_push_jobs_status ON cache_push_jobs (status);

CREATE INDEX IF NOT EXISTS idx_cache_push_jobs_derivation_id ON cache_push_jobs (derivation_id);

CREATE INDEX IF NOT EXISTS idx_cache_push_jobs_scheduled_at ON cache_push_jobs (scheduled_at);

-- Add unique constraint for pending/in_progress jobs
CREATE UNIQUE INDEX IF NOT EXISTS idx_cache_push_jobs_derivation_unique ON cache_push_jobs (derivation_id)
WHERE
    status IN ('pending', 'in_progress');

