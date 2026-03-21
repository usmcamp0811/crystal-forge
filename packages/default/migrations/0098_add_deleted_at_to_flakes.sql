-- Add soft delete support to flakes table
-- Migration: 0098_add_deleted_at_to_flakes.sql

-- Add deleted_at column for soft delete
ALTER TABLE flakes
ADD COLUMN deleted_at TIMESTAMPTZ NULL;

-- Add index for filtering out deleted flakes in queries
CREATE INDEX idx_flakes_deleted_at ON flakes(deleted_at);

-- Comment explaining the soft delete pattern
COMMENT ON COLUMN flakes.deleted_at IS 'Timestamp when flake was soft-deleted. NULL means active, non-NULL means deleted.';
