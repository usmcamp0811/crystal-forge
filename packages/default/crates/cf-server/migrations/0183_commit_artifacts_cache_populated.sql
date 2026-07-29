-- Add nixos_configurations_populated column to distinguish a legitimately
-- empty configuration set from a failed hydration attempt.
--
-- Prior to this migration, mark_commit_artifact_hydration_failed wrote empty
-- arrays into nixos_configurations and get_commit_nixos_configurations_from_cache
-- could not tell the difference between "successfully discovered zero systems"
-- and "discovery never completed". This caused the evaluator to skip inline
-- discovery whenever a background hydration had previously failed for the same
-- commit.
--
-- The semantics of the new column:
--   TRUE  (default) → nixos_configurations was populated by a successful
--                      discovery or hydration run.
--   FALSE            → the last hydration attempt failed; inline discovery
--                      should be retried.

ALTER TABLE commit_artifacts_cache
ADD COLUMN nixos_configurations_populated BOOLEAN NOT NULL DEFAULT TRUE;

-- Backfill existing rows that have empty arrays for both columns.
-- These were written by mark_commit_artifact_hydration_failed prior to the
-- addition of nixos_configurations_populated, and are indistinguishable from
-- a legitimately empty discovery without this heuristic.
UPDATE commit_artifacts_cache
SET nixos_configurations_populated = FALSE
WHERE COALESCE(cardinality(nixos_configurations), 0) = 0
  AND COALESCE(cardinality(changed_files), 0) = 0;
