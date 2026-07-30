-- Fix backfill order: 0192 used created_at DESC, which assigned position 1
-- to the newest job. Coupled with ORDER BY queue_position DESC this placed
-- the oldest job first — the opposite of LIFO.
--
-- This migration re-backfills any null queue_position values with ASC ordering
-- so that oldest → position 1, newest → position N, and DESC sort picks the
-- newest first (correct LIFO).

WITH ordered AS (
    SELECT id,
           ROW_NUMBER() OVER (ORDER BY created_at ASC, id ASC) AS seq
    FROM build_jobs
    WHERE status = 'queued'
)
UPDATE build_jobs
SET queue_position = o.seq
FROM ordered o
WHERE build_jobs.id = o.id
  AND build_jobs.queue_position IS NULL;
