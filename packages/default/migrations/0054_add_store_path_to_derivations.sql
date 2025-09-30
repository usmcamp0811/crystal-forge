-- Add store_path column to derivations table
-- This stores the actual built output path (persists in Nix store)
-- while derivation_path stores the .drv path (may be GC'd)
ALTER TABLE derivations
    ADD COLUMN store_path text;

-- Add index for efficient queries on store_path
CREATE INDEX idx_derivations_store_path ON derivations (store_path)
WHERE
    store_path IS NOT NULL;

-- Add comment explaining the field
COMMENT ON COLUMN derivations.store_path IS 'The actual built output path in the Nix store. Used for cache push operations. Persists after GC unlike derivation_path (.drv files).';

