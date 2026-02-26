-- Migration 0080: Environment Policies
-- Environment policies serve as the baseline for systems.
-- Systems can add additional policies but cannot remove the baseline.

-- Master list of available deployment policies
CREATE TABLE IF NOT EXISTS deployment_policies (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid (),
    name text NOT NULL UNIQUE,
    description text,
    -- The policy type determines how it's evaluated
    policy_type text NOT NULL, -- e.g., 'require_cf_agent', 'require_packages', 'custom_check'
    -- JSON blob containing type-specific configuration
    -- For 'require_cf_agent': { "strict": true }
    -- For 'require_packages': { "packages": ["vim", "git"], "strict": true }
    -- For 'custom_check': { "expression": "config.networking.firewall.enable", "description": "Firewall enabled" }
    config jsonb NOT NULL DEFAULT '{}',
    enabled boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Create index for looking up policies by name
CREATE INDEX idx_deployment_policies_name ON deployment_policies (name);

-- Create update trigger
CREATE TRIGGER trigger_deployment_policies_updated_at
    BEFORE UPDATE ON deployment_policies
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column ();

-- Baseline policies required by each environment
-- These are MANDATORY for all systems in the environment
CREATE TABLE IF NOT EXISTS environment_policies (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid (),
    environment_id uuid NOT NULL REFERENCES environments (id) ON DELETE CASCADE,
    policy_id uuid NOT NULL REFERENCES deployment_policies (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by uuid REFERENCES users (id),
    UNIQUE(environment_id, policy_id)
);

-- Index for fast lookup of environment's required policies
CREATE INDEX idx_environment_policies_env ON environment_policies (environment_id);

-- Additional policies that can be assigned to specific systems
-- These are OPTIONAL and ADD ON TOP of the environment's baseline
-- A system can have these in addition to its environment's baseline policies
CREATE TABLE IF NOT EXISTS system_policies (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid (),
    system_id uuid NOT NULL REFERENCES systems (id) ON DELETE CASCADE,
    policy_id uuid NOT NULL REFERENCES deployment_policies (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by uuid REFERENCES users (id),
    UNIQUE(system_id, policy_id)
);

-- Index for fast lookup of system's additional policies
CREATE INDEX idx_system_policies_system ON system_policies (system_id);

-- Insert default deployment policies
INSERT INTO deployment_policies (name, description, policy_type, config)
VALUES 
    ('require_cf_agent', 'Require Crystal Forge agent to be enabled', 'require_cf_agent', '{"strict": true}'::jsonb),
    ('require_firewall', 'Require firewall to be enabled', 'custom_check', '{"expression": "config.networking.firewall.enable", "description": "Firewall must be enabled", "strict": true}'::jsonb),
    ('require_ssh_key_auth', 'Require SSH key-only authentication', 'custom_check', '{"expression": "!config.services.openssh.settings.PasswordAuthentication", "description": "Password authentication must be disabled", "strict": false}'::jsonb),
    ('require_auditd', 'Require audit daemon', 'custom_check', '{"expression": "config.services.auditd.enable or false", "description": "Audit daemon should be enabled", "strict": false}'::jsonb)
ON CONFLICT (name) DO NOTHING;

-- Grant necessary permissions (adjust as needed for your setup)
-- GRANT SELECT ON deployment_policies TO authenticated;
-- GRANT SELECT, INSERT, DELETE ON environment_policies TO authenticated;
-- GRANT SELECT, INSERT, DELETE ON system_policies TO authenticated;
