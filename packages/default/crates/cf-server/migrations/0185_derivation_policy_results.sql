ALTER TABLE derivations
    ADD COLUMN IF NOT EXISTS policy_requirements_met BOOLEAN,
    ADD COLUMN IF NOT EXISTS policy_results JSONB NOT NULL DEFAULT '{}'::jsonb;

UPDATE derivations
SET policy_requirements_met = cf_agent_enabled
WHERE policy_requirements_met IS NULL
  AND cf_agent_enabled IS NOT NULL;
