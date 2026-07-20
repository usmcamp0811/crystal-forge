-- Migration 0144: Track desired-target freshness independently from row updates
--
-- systems.updated_at changes for many unrelated reasons, so it must not be
-- used to decide whether a manual deployment request is fresh enough for an
-- agent to consume. Existing manual desired targets are intentionally left
-- without freshness metadata so they are suppressed until explicitly set again.

ALTER TABLE systems
    ADD COLUMN IF NOT EXISTS desired_target_set_at timestamptz;

UPDATE systems
SET desired_target_set_at = updated_at
WHERE desired_target IS NOT NULL
  AND deployment_policy <> 'manual'
  AND desired_target_set_at IS NULL;

UPDATE systems
SET desired_target_set_at = NULL
WHERE desired_target IS NULL
   OR deployment_policy = 'manual';

CREATE INDEX IF NOT EXISTS idx_systems_desired_target_set_at
    ON systems(desired_target_set_at)
    WHERE desired_target IS NOT NULL;
