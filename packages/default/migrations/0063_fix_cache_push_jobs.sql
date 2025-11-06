-- Migration: Add retry logic to cache_push_jobs
-- Adds exponential backoff for failed jobs
-- Add retry_after column to track when a failed job should be retried
ALTER TABLE cache_push_jobs
    ADD COLUMN IF NOT EXISTS retry_after timestamp;

-- Add index to speed up queries for retryable jobs
CREATE INDEX IF NOT EXISTS idx_cache_push_jobs_retry ON cache_push_jobs (status, retry_after)
WHERE
    status = 'failed' AND retry_after IS NOT NULL;

-- Add index for pending jobs
CREATE INDEX IF NOT EXISTS idx_cache_push_jobs_pending ON cache_push_jobs (status, scheduled_at)
WHERE
    status = 'pending';

-- Optional: Add comment explaining the retry logic
COMMENT ON COLUMN cache_push_jobs.retry_after IS 'Timestamp when a failed job should be retried. NULL for permanently failed jobs (attempts >= 5) or non-failed jobs.';

