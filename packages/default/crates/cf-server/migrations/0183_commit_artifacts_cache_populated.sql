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

-- Existing rows that were written by mark_commit_artifact_hydration_failed
-- have nixos_configurations = '{}' and populated_at set. There is no reliable
-- way to distinguish them from a legitimately empty-but-successful discovery
-- at this point, so they keep the default TRUE. Only future hydration failures
-- will set FALSE, making the distinction meaningful going forward.
