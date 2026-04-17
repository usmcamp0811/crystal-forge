-- Add evaluation cancellation support.
--
-- Extends the commits table to support two new evaluation statuses:
--   - 'cancelling': an in_progress eval has been requested to cancel;
--     the eval loop will detect this flag and kill the subprocess.
--   - 'cancelled': the eval was cleanly cancelled (either from pending or
--     after the loop detected cancellation_requested).
--
-- Also adds a cooperative cancellation flag polled by the eval loop, and
-- rebuilds the unique partial index to also enforce at-most-one 'cancelling'
-- eval at any time (mirrors the existing in_progress constraint).
--
-- Does not modify any previous migration.

-- Step 1: Extend the CHECK constraint on evaluation_status.
-- Drop the old constraint (if any) and recreate with the two new values.
ALTER TABLE commits DROP CONSTRAINT IF EXISTS commits_evaluation_status_check;
ALTER TABLE commits
    ADD CONSTRAINT commits_evaluation_status_check
    CHECK (evaluation_status IN (
        'pending', 'in_progress', 'cancelling', 'cancelled', 'complete', 'failed'
    ));

-- Step 2: Add cooperative cancellation flag.
-- The eval loop polls this column every ~2s while a subprocess is running.
-- When set to TRUE, the loop kills the subprocess and transitions to 'cancelled'.
ALTER TABLE commits
    ADD COLUMN IF NOT EXISTS cancellation_requested BOOLEAN NOT NULL DEFAULT FALSE;

-- Step 3: Rebuild the single-in-progress index to enforce at-most-one active eval
-- across BOTH 'in_progress' and 'cancelling' states.
--
-- The previous index (migration 0088) indexed ON commits (evaluation_status)
-- WHERE evaluation_status = 'in_progress'. Uniqueness was enforced per value,
-- so a simple IN ('in_progress','cancelling') predicate on the same column would
-- still allow one in_progress row AND one cancelling row simultaneously.
--
-- Fix: index a constant expression ((1)) so the partial index contains exactly
-- one entry whenever any qualifying row exists, regardless of which status value
-- that row holds. This guarantees at most one row total can be in either state.
DROP INDEX IF EXISTS idx_commits_single_in_progress;
CREATE UNIQUE INDEX idx_commits_single_in_progress
    ON commits ((1))
    WHERE evaluation_status IN ('in_progress', 'cancelling');
