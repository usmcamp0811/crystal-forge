-- Fix the queue_position backfill: 0192 used ORDER BY created_at DESC, which
-- assigned position 1 to the newest queued build.  With ORDER BY queue_position
-- DESC that made the oldest build first — the opposite of LIFO.
--
-- Reassign so that:
--   oldest queued build → position 1
--   newest             → position N
-- ORDER BY queue_position DESC then correctly places the newest item first.

WITH ordered AS (
    SELECT id,
           ROW_NUMBER() OVER (ORDER BY created_at ASC, id ASC) AS seq
    FROM build_jobs
    WHERE status = 'queued'
)
UPDATE build_jobs
SET queue_position = o.seq
FROM ordered o
WHERE build_jobs.id = o.id;
