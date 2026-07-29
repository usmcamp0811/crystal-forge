-- Migration 0184: enforce one build_jobs row per derivation.
--
-- Product semantics: build_jobs is a mutable state-machine row. A failed job
-- is re-queued in-place (status -> 'queued', retry_count++). A new row is
-- never created for a retry. Therefore at most one row per derivation is
-- meaningful. A global UNIQUE index on derivation_id is correct.
--
-- Canonical-row selection — two-pass priority:
--   Active work first:
--     building   rank 1  -- active builder; must not be displaced
--     queued     rank 2  -- waiting for a builder slot
--     cancelling rank 3  -- stop requested, builder still running
--   Best terminal state:
--     success    rank 4  -- final good outcome; preferred over prior failure
--     cancelled  rank 5  -- clean stop; preferred over stale failure
--     failed     rank 6  -- terminal failure; only if no better row exists
--   Ties broken by earlier created_at, then lower UUID.
--
-- Rationale for success(4) < failed(6): if duplicate rows exist and one
-- records a successful build, that row has the valid output path and must be
-- retained. A prior failed row from the same derivation is superseded.
--
-- Non-FK logical reference: attention_occurrences.subject_id stores build_job
-- UUIDs as text strings. Deleting a duplicate row whose UUID appears in
-- attention_occurrences leaves a dangling string reference. This is harmless:
-- the reconciliation task queries build_jobs by UUID before reopening an
-- occurrence; a deleted row will not be found and no occurrence will be
-- reopened. Existing open occurrences age out through normal GC.

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
                    WHEN 'success'    THEN 4
                    WHEN 'cancelled'  THEN 5
                    WHEN 'failed'     THEN 6
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
