-- Migration 1: Add deployment columns to systems table
-- Add deployment-related columns to the systems table
ALTER TABLE public.systems
    ADD COLUMN desired_target text,
    ADD COLUMN deployment_policy text DEFAULT 'manual' CHECK (deployment_policy IN ('manual', 'auto_latest', 'pinned'));

-- Migration 2: Add cf_agent_enabled column to derivations table
-- Tracks whether a derivation has Crystal Forge client enabled
ALTER TABLE public.derivations
    ADD COLUMN cf_agent_enabled boolean DEFAULT FALSE;

-- Migration 3: Update system_states change_reason constraint
-- Add 'cf_deployment' as a valid change reason for agent-initiated deployments
-- First, drop the existing constraint
ALTER TABLE public.system_states
    DROP CONSTRAINT valid_change_reason;

-- Add the new constraint with 'cf_deployment' included
ALTER TABLE public.system_states
    ADD CONSTRAINT valid_change_reason CHECK (change_reason = ANY (ARRAY['startup'::text, 'config_change'::text, 'state_delta'::text, 'cf_deployment'::text]));

-- Migration 4: Add indexes for deployment performance
-- Add indexes to support efficient deployment queries
CREATE INDEX idx_systems_deployment_policy ON public.systems (deployment_policy);

CREATE INDEX idx_systems_desired_target ON public.systems (desired_target);

CREATE INDEX idx_derivations_cf_agent_enabled ON public.derivations (cf_agent_enabled);

CREATE INDEX idx_system_states_change_reason ON public.system_states (change_reason);

-- Migration 5: Add foreign key constraint for desired_target (optional)
-- This ensures desired_target references a valid derivation path
-- Note: This constraint may fail if there are existing invalid references
-- Uncomment the following lines if you want to enforce referential integrity:
ALTER TABLE public.systems
    ADD CONSTRAINT fk_systems_desired_target FOREIGN KEY (desired_target) REFERENCES public.derivations (derivation_path);

