-- Existing enrolled systems retain managed authority. Configuration assurance
-- does not itself grant agent authentication or deployment authority.
ALTER TABLE systems
    ADD COLUMN management_mode text NOT NULL DEFAULT 'managed',
    ADD COLUMN parent_host_id uuid REFERENCES systems(id),
    ALTER COLUMN public_key DROP NOT NULL,
    ADD CONSTRAINT systems_management_mode_check
        CHECK (management_mode IN ('managed', 'external', 'reference')),
    ADD CONSTRAINT systems_management_mode_key_check
        CHECK (
            (management_mode = 'managed' AND public_key IS NOT NULL
                AND public_key !~ '^[[:space:]]*$')
            OR (management_mode IN ('external', 'reference') AND public_key IS NULL)
        ),
    ADD CONSTRAINT systems_management_mode_target_check
        CHECK (management_mode = 'managed' OR (
            desired_target IS NULL
            AND desired_derivation_id IS NULL
            AND desired_target_set_at IS NULL
        )),
    ADD CONSTRAINT systems_parent_host_check
        CHECK (parent_host_id IS NULL OR (
            management_mode = 'external' AND parent_host_id <> id
        ));

COMMENT ON COLUMN systems.management_mode IS
    'Deployment authority: managed uses an enrolled agent; external is deployed outside Crystal Forge; reference is a configuration, not a machine. Independent of is_active and configuration evidence.';
COMMENT ON COLUMN systems.public_key IS
    'Validated agent enrollment key for managed systems only. External and reference resources have NULL keys and cannot authenticate as agents.';
COMMENT ON COLUMN systems.parent_host_id IS
    'Optional parent association for external resources only. Does not prove runtime state or authorize parent reports; authorization and environment checks belong to the server.';
