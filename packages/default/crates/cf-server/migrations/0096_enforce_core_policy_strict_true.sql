-- Migration 0096: Enforce strict=true for core require_cf_agent policy at DB level.
--
-- This is defense in depth for API/runtime enforcement. The core policy must
-- never be weakened by persisting strict=false or omitting strict entirely.

-- Normalize any legacy core policy rows before adding the constraint.
UPDATE deployment_policies
SET config = jsonb_set(COALESCE(config, '{}'::jsonb), '{strict}', 'true'::jsonb, true),
    updated_at = CURRENT_TIMESTAMP
WHERE policy_type = 'require_cf_agent';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'deployment_policies_require_cf_agent_strict_true'
    ) THEN
        ALTER TABLE deployment_policies
        ADD CONSTRAINT deployment_policies_require_cf_agent_strict_true
        CHECK (
            policy_type <> 'require_cf_agent'
            OR (
                jsonb_typeof(config) = 'object'
                AND config ? 'strict'
                AND config->'strict' = 'true'::jsonb
            )
        );
    END IF;
END
$$;
