-- Add canary_rollout_state table for tracking phased deployment progress
-- Supports canary_rollout policy type

CREATE TABLE canary_rollout_state (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- What is being rolled out
    rollout_context_type text NOT NULL CHECK (rollout_context_type IN ('commit', 'derivation')),
    rollout_context_id text NOT NULL,  -- commit SHA or derivation ID
    
    -- Policy driving this rollout
    policy_id uuid NOT NULL REFERENCES deployment_policies(id) ON DELETE CASCADE,
    
    -- Rollout progress
    current_phase integer NOT NULL DEFAULT 1,  -- Which phase we're in (1-based)
    total_phases integer NOT NULL,  -- Total number of phases (based on percentage)
    phase_started_at timestamptz NOT NULL DEFAULT now(),
    phase_observation_end timestamptz,  -- When observation period ends for current phase
    
    -- System selection
    systems_in_current_phase uuid[] NOT NULL DEFAULT '{}',  -- Array of system IDs in current phase
    systems_completed uuid[] NOT NULL DEFAULT '{}',  -- Array of system IDs that completed successfully
    systems_failed uuid[] NOT NULL DEFAULT '{}',  -- Array of system IDs that failed health checks
    
    -- Rollout status
    status text NOT NULL DEFAULT 'in_progress' CHECK (status IN ('in_progress', 'observing', 'completed', 'failed', 'halted')),
    halted_reason text,  -- Why rollout was halted (if status = 'halted')
    
    -- Timestamps
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    
    UNIQUE(rollout_context_type, rollout_context_id, policy_id)
);

CREATE INDEX idx_canary_rollout_context ON canary_rollout_state(rollout_context_type, rollout_context_id);
CREATE INDEX idx_canary_rollout_policy ON canary_rollout_state(policy_id);
CREATE INDEX idx_canary_rollout_status ON canary_rollout_state(status);
CREATE INDEX idx_canary_rollout_observation_end ON canary_rollout_state(phase_observation_end) WHERE status = 'observing';

COMMENT ON TABLE canary_rollout_state IS 'Tracks canary rollout progress for phased deployment policies';
COMMENT ON COLUMN canary_rollout_state.current_phase IS 'Current phase number (1-based); increments after each observation period';
COMMENT ON COLUMN canary_rollout_state.total_phases IS 'Total phases calculated from percentage (e.g., 25% = 4 phases)';
COMMENT ON COLUMN canary_rollout_state.phase_observation_end IS 'When current observation period ends; NULL if not observing';
COMMENT ON COLUMN canary_rollout_state.status IS 'in_progress: deploying to phase, observing: waiting after deployment, completed: all phases done, failed: errors occurred, halted: manually stopped or health check failed';
