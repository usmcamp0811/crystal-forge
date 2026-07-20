-- Add soft delete support to flakes table
-- Migration: 0098_add_deleted_at_to_flakes.sql

-- Add deleted_at column for soft delete (idempotent)
ALTER TABLE flakes
ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ NULL;

-- Drop old index if it exists, then create partial index for active flakes
-- This index only covers rows where deleted_at IS NULL, which is much more
-- efficient than indexing all rows including deleted ones
DROP INDEX IF EXISTS idx_flakes_deleted_at;
CREATE INDEX IF NOT EXISTS idx_flakes_active ON flakes(id, name, repo_url) WHERE deleted_at IS NULL;

-- Comment explaining the soft delete pattern
COMMENT ON COLUMN flakes.deleted_at IS 'Timestamp when flake was soft-deleted. NULL means active, non-NULL means deleted.';
