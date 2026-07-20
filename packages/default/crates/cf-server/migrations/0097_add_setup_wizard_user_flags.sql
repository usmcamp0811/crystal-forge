ALTER TABLE users
ADD COLUMN IF NOT EXISTS setup_wizard_dismissed boolean NOT NULL DEFAULT false;

ALTER TABLE users
ADD COLUMN IF NOT EXISTS setup_wizard_agent_acknowledged boolean NOT NULL DEFAULT false;

COMMENT ON COLUMN users.setup_wizard_dismissed IS 'Whether onboarding setup wizard is dismissed/completed for this user.';
COMMENT ON COLUMN users.setup_wizard_agent_acknowledged IS 'Whether the onboarding wizard agent deployment informational step was acknowledged.';
