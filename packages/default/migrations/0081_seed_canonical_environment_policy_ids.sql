-- Migration 0081: Seed canonical policy IDs expected by environments UI
--
-- The current environments UI uses stable UUID values for policy selection.
-- Seed those canonical IDs so PATCH /api/v1/environments/:id/policies can persist
-- without foreign key failures.

INSERT INTO deployment_policies (id, name, description, policy_type, config, enabled)
VALUES
    (
        '00000000-0000-0000-0000-000000000001'::uuid,
        'Require Crystal Forge Agent',
        'Ensure Crystal Forge services are enabled on the target.',
        'require_cf_agent',
        '{"strict": true}'::jsonb,
        true
    ),
    (
        '00000000-0000-0000-0000-000000000002'::uuid,
        'Require Packages',
        'Guarantee required package set is installed.',
        'require_packages',
        '{"packages": [], "strict": true}'::jsonb,
        true
    ),
    (
        '00000000-0000-0000-0000-000000000003'::uuid,
        'Custom Check',
        'Evaluate environment-specific Nix policy expression.',
        'custom_check',
        '{"expression": "true", "description": "Custom policy", "strict": false}'::jsonb,
        true
    )
ON CONFLICT (id) DO NOTHING;
