-- Migration 0089: Enforce single immutable core require_cf_agent policy
--
-- Previous migrations seeded both a lowercase-name policy and a canonical-id policy,
-- which can result in duplicate require_cf_agent entries. Consolidate them to one
-- canonical row and enforce future uniqueness at the database level.

DO $$
DECLARE
    core_policy_id CONSTANT uuid := '00000000-0000-0000-0000-000000000001'::uuid;
BEGIN
    -- Ensure canonical core policy row exists and is enabled.
    INSERT INTO deployment_policies (id, name, description, policy_type, config, enabled)
    VALUES (
        core_policy_id,
        'Require Crystal Forge Agent',
        'Ensure Crystal Forge services are enabled on the target.',
        'require_cf_agent',
        '{"strict": true}'::jsonb,
        true
    )
    ON CONFLICT (id) DO UPDATE
    SET
        name = EXCLUDED.name,
        description = EXCLUDED.description,
        policy_type = 'require_cf_agent',
        config = EXCLUDED.config,
        enabled = true,
        updated_at = CURRENT_TIMESTAMP;

    -- Move environment policy references from duplicate require_cf_agent rows to canonical.
    INSERT INTO environment_policies (environment_id, policy_id, created_at, created_by)
    SELECT ep.environment_id, core_policy_id, ep.created_at, ep.created_by
    FROM environment_policies ep
    JOIN deployment_policies dp ON dp.id = ep.policy_id
    WHERE dp.policy_type = 'require_cf_agent'
      AND dp.id <> core_policy_id
    ON CONFLICT (environment_id, policy_id) DO NOTHING;

    -- Move system policy references from duplicate require_cf_agent rows to canonical.
    INSERT INTO system_policies (system_id, policy_id, created_at, created_by)
    SELECT sp.system_id, core_policy_id, sp.created_at, sp.created_by
    FROM system_policies sp
    JOIN deployment_policies dp ON dp.id = sp.policy_id
    WHERE dp.policy_type = 'require_cf_agent'
      AND dp.id <> core_policy_id
    ON CONFLICT (system_id, policy_id) DO NOTHING;

    -- Remove old references to duplicate require_cf_agent rows.
    DELETE FROM environment_policies
    WHERE policy_id IN (
        SELECT id
        FROM deployment_policies
        WHERE policy_type = 'require_cf_agent'
          AND id <> core_policy_id
    );

    DELETE FROM system_policies
    WHERE policy_id IN (
        SELECT id
        FROM deployment_policies
        WHERE policy_type = 'require_cf_agent'
          AND id <> core_policy_id
    );

    -- Delete duplicate require_cf_agent rows, keeping only canonical row.
    DELETE FROM deployment_policies
    WHERE policy_type = 'require_cf_agent'
      AND id <> core_policy_id;

    -- Ensure canonical row remains enabled.
    UPDATE deployment_policies
    SET enabled = true,
        updated_at = CURRENT_TIMESTAMP
    WHERE id = core_policy_id;
END
$$;

-- Enforce exactly one require_cf_agent policy row by policy_type.
CREATE UNIQUE INDEX IF NOT EXISTS idx_deployment_policies_single_core_cf_agent
    ON deployment_policies (policy_type)
    WHERE policy_type = 'require_cf_agent';

-- Enforce require_cf_agent is always enabled.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'deployment_policies_require_cf_agent_enabled'
    ) THEN
        ALTER TABLE deployment_policies
        ADD CONSTRAINT deployment_policies_require_cf_agent_enabled
        CHECK (policy_type <> 'require_cf_agent' OR enabled = true);
    END IF;
END
$$;
