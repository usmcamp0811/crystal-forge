-- Reassign queued build queue_position values after 0192/0193.
--
-- 0192 filled existing queued rows using DESC order, which assigned the newest
-- job the lowest position. 0193 only touched NULL queue_position values for
-- checksum compatibility with already-applied deployments. This migration
-- updates all currently queued rows so ORDER BY queue_position DESC is LIFO.

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
