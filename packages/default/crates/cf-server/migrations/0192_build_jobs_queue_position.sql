-- Add a durable queue position field to build_jobs for LIFO ordering.
-- Higher position = earlier execution (ORDER BY queue_position DESC).
-- New jobs get COALESCE(MAX(queue_position), 0) + 1 (tail append).
-- Combined with DESC sort this produces LIFO: newest job = highest position = first.

ALTER TABLE build_jobs ADD COLUMN IF NOT EXISTS queue_position bigint;

-- Backfill existing queued builds with LIFO ordering.
-- The most recently queued build gets the highest position so that
-- ORDER BY queue_position DESC picks it first.
WITH ordered AS (
    SELECT id,
           ROW_NUMBER() OVER (ORDER BY created_at DESC, id DESC) AS seq
    FROM build_jobs
    WHERE status = 'queued'
)
UPDATE build_jobs
SET queue_position = o.seq
FROM ordered o
WHERE build_jobs.id = o.id
  AND build_jobs.queue_position IS NULL;

-- Index for the claim query: queued jobs ordered by queue_position DESC.
CREATE INDEX IF NOT EXISTS idx_build_jobs_queue_order
    ON build_jobs (queue_position DESC NULLS LAST)
    WHERE status = 'queued';
