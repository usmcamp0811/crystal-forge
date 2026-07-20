-- Add deployment_approvals table for tracking policy-required approvals
-- Supports require_approvals policy type

CREATE TABLE deployment_approvals (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- What is being approved
    deployment_context_type text NOT NULL CHECK (deployment_context_type IN ('commit', 'derivation', 'system_deployment')),
    deployment_context_id text NOT NULL,  -- commit SHA, derivation ID, or system deployment ID
    
    -- Policy that required this approval
    policy_id uuid NOT NULL REFERENCES deployment_policies(id) ON DELETE CASCADE,
    
    -- Who approved
    approved_by uuid NOT NULL REFERENCES users(id),
    approved_at timestamptz NOT NULL DEFAULT now(),
    
    -- Approval metadata
    comment text,  -- Optional approval comment
    
    -- Expiration
    expires_at timestamptz,  -- NULL = never expires
    
    UNIQUE(deployment_context_type, deployment_context_id, policy_id, approved_by)
);

CREATE INDEX idx_deployment_approvals_context ON deployment_approvals(deployment_context_type, deployment_context_id);
CREATE INDEX idx_deployment_approvals_policy ON deployment_approvals(policy_id);
CREATE INDEX idx_deployment_approvals_expires ON deployment_approvals(expires_at) WHERE expires_at IS NOT NULL;

COMMENT ON TABLE deployment_approvals IS 'Tracks operator approvals for deployment policies requiring manual sign-off';
COMMENT ON COLUMN deployment_approvals.deployment_context_type IS 'Type of deployment: commit (flake evaluation), derivation (build), or system_deployment (specific system update)';
COMMENT ON COLUMN deployment_approvals.deployment_context_id IS 'ID of the deployment context (commit SHA, derivation ID, system deployment ID)';
COMMENT ON COLUMN deployment_approvals.expires_at IS 'When this approval expires; NULL means never expires';
