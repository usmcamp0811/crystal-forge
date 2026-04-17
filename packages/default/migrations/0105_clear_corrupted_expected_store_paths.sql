-- Clear expected_store_path values that may have been corrupted by the old
-- broad JSON scanning parser. These will be repopulated correctly on next eval
-- using the fixed nix-store --query --outputs approach.
--
-- We only clear expected_store_path where store_path is NULL (not yet built)
-- because:
-- 1. Built derivations already have store_path set from the actual build
-- 2. Only the expected_store_path from eval phase could be corrupted
-- 3. After rebuild, the correct store_path takes precedence anyway
--
-- Derivations with non-NULL store_path are unaffected because:
-- - COALESCE(store_path, expected_store_path) will use store_path first
-- - store_path is set from actual build output, which is always correct

UPDATE derivations
SET expected_store_path = NULL
WHERE store_path IS NULL
  AND expected_store_path IS NOT NULL;

-- Log how many rows were affected (for debugging)
-- SELECT COUNT(*) FROM derivations WHERE store_path IS NULL AND expected_store_path IS NOT NULL;
