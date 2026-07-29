-- Add durable build-preparation state to derivations so the recovery
-- reconciler can distinguish an activation failure from an intentionally
-- non-buildable derivation (build_scope exclusion, policy failure, etc.).
--
-- State machine:
--   NULL            – row predates this migration or derivation is not build-eligible.
--   'not_required'  – derivation was persisted with RecordedWithoutBuild (scope excluded,
--                     policy failed, cancellation, etc.). Must never be recovered.
--   'pending'       – NeedsBuildPreparation: GC-root creation and build-job activation
--                     have been scheduled but have not completed yet. Recovery target.
--   'failed'        – Preparation task completed with an error (GC root or activation).
--                     Recovery target for retryable failures.
--   'queued'        – Build job was successfully activated and is claimable.
--
-- The column is NULL for all pre-existing rows so the recovery query can
-- ignore them safely: NULL rows were persisted before this state machine
-- existed and may have no build job for legitimate reasons.

ALTER TABLE derivations
    ADD COLUMN IF NOT EXISTS build_preparation_state text
    CONSTRAINT derivations_build_preparation_state_check
    CHECK (build_preparation_state IN ('not_required', 'pending', 'failed', 'queued'));

-- Index to make the recovery query fast: only 'pending' and 'failed' rows
-- with DryRunComplete and eligible flags need to be scanned.
CREATE INDEX IF NOT EXISTS idx_derivations_build_prep_recovery
    ON derivations (build_preparation_state)
    WHERE build_preparation_state IN ('pending', 'failed')
      AND status_id = 5
      AND cf_agent_enabled = TRUE
      AND policy_requirements_met = TRUE;
