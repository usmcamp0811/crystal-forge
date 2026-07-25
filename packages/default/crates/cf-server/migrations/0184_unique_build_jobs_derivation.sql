-- Migration 0184: enforce one build_jobs row per derivation.
--
-- Product semantics: build_jobs is a mutable state-machine row. A failed job is
-- re-queued in-place (status -> 'queued', retry_count++). A new row is never
-- created for a retry. Therefore at most one row per derivation is meaningful.
-- A global UNIQUE index on derivation_id is correct.
--
-- Canonical-row selection: prefer the row in the most advanced lifecycle state
-- (building > queued > cancelling > cancelled > failed > success), breaking
-- ties by earlier created_at then lower id. This keeps any active work and
-- avoids demoting a running build.
--
-- No other table references build_jobs with a foreign key (verified via
-- pg_constraint scan), so row deletion cannot break referential integrity.

WITH ranked AS (
    SELECT
        id,
        derivation_id,
        ROW_NUMBER() OVER (
            PARTITION BY derivation_id
            ORDER BY
                CASE status
                    WHEN 'building'   THEN 1
                    WHEN 'queued'     THEN 2
                    WHEN 'cancelling' THEN 3
                    WHEN 'cancelled'  THEN 4
                    WHEN 'failed'     THEN 5
                    WHEN 'success'    THEN 6
                    ELSE                   7
                END,
                created_at ASC,
                id ASC
        ) AS rn
    FROM build_jobs
    WHERE derivation_id IS NOT NULL
)
DELETE FROM build_jobs
WHERE id IN (
    SELECT id FROM ranked WHERE rn > 1
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_build_jobs_derivation_unique
    ON build_jobs (derivation_id);
