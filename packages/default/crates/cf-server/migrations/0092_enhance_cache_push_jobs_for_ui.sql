-- Enhance cache_push_jobs to support UI cache management features
-- This migration adds new statuses and improves the cache_destination relationship

-- Step 1: Add new status values to the CHECK constraint
-- We need to drop and recreate the constraint to add 'cancelled' and 'permanently_failed'
ALTER TABLE cache_push_jobs
    DROP CONSTRAINT IF EXISTS cache_push_jobs_status_check;

ALTER TABLE cache_push_jobs
    ADD CONSTRAINT cache_push_jobs_status_check 
    CHECK (status IN ('pending', 'in_progress', 'completed', 'failed', 'cancelled', 'permanently_failed'));

-- Step 2: Add index for cache_destination lookups (for filtering by destination)
CREATE INDEX IF NOT EXISTS idx_cache_push_jobs_cache_destination 
    ON cache_push_jobs(cache_destination) 
    WHERE cache_destination IS NOT NULL;

-- Step 3: Add index for completed jobs with timestamps (for reporting)
CREATE INDEX IF NOT EXISTS idx_cache_push_jobs_completed 
    ON cache_push_jobs(completed_at, status) 
    WHERE status IN ('completed', 'permanently_failed');

-- Note: We keep cache_destination as TEXT (not a foreign key) for flexibility
-- This allows:
-- 1. Backward compatibility with existing jobs that reference server.toml config
-- 2. Jobs to complete even if cache destination is later deleted
-- 3. Historical tracking of which cache was used even after cache config changes
--
-- The application layer will join with cache_destinations table when the name matches,
-- but will gracefully handle cases where the destination no longer exists.

-- Add helpful comment
COMMENT ON COLUMN cache_push_jobs.cache_destination IS 
    'Name of cache destination used for this job. May reference cache_destinations.name or be a legacy value from server.toml. NULL means use default cache config.';
