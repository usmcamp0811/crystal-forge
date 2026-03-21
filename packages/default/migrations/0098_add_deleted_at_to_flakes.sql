-- Add soft delete support to flakes table
-- Migration: 0098_add_deleted_at_to_flakes.sql

-- Add deleted_at column for soft delete
ALTER TABLE flakes
ADD COLUMN deleted_at TIMESTAMPTZ NULL;

-- Add partial index for active flakes (the common query path)
-- This index only covers rows where deleted_at IS NULL, which is much more
-- efficient than indexing all rows including deleted ones
CREATE INDEX idx_flakes_active ON flakes(id, name, repo_url) WHERE deleted_at IS NULL;

-- Comment explaining the soft delete pattern
COMMENT ON COLUMN flakes.deleted_at IS 'Timestamp when flake was soft-deleted. NULL means active, non-NULL means deleted.';
