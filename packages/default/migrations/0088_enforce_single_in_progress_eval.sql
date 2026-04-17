-- Migration: Enforce single in-progress evaluation at a time
-- This ensures only one commit can be actively evaluating at once

-- Step 1: Reset ALL in_progress commits to pending (startup cleanup)
-- This handles the case where multiple in_progress states exist from previous runs
UPDATE commits
SET 
    evaluation_status = 'pending',
    evaluation_started_at = NULL
WHERE evaluation_status = 'in_progress';

-- Step 2: Create a unique partial index to enforce single in-progress evaluation
-- This prevents multiple commits from being marked as in_progress simultaneously
-- Note: Cannot use CONCURRENTLY in a migration transaction, using regular CREATE INDEX
CREATE UNIQUE INDEX idx_commits_single_in_progress
ON commits (evaluation_status)
WHERE evaluation_status = 'in_progress';

-- This index ensures that only one row can have evaluation_status = 'in_progress' at any time
-- Attempts to set a second commit to in_progress will fail with a unique constraint violation
