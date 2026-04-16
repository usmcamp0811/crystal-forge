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

-- Step 3: Rebuild the single-in-progress index to also cover 'cancelling'.
-- The original index (migration 0088) only covers evaluation_status = 'in_progress',
-- which prevents two concurrent evaluations. We extend it so that 'cancelling'
-- is also covered — only one commit can be in either state at a time.
DROP INDEX IF EXISTS idx_commits_single_in_progress;
CREATE UNIQUE INDEX idx_commits_single_in_progress
    ON commits (evaluation_status)
    WHERE evaluation_status IN ('in_progress', 'cancelling');
