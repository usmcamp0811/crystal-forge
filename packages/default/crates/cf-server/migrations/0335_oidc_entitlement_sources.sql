-- Effective rows retain their original IDs and actor attribution. Ownership of
-- pre-existing grants for externally linked users cannot be inferred safely.
-- Reserve globally ordered generations before redirect, not at callback time.
-- CACHE 1 preserves reservation order across database sessions; never cycle.
CREATE SEQUENCE oidc_entitlement_generation_seq AS bigint MINVALUE 1 NO CYCLE CACHE 1;

ALTER TABLE external_identities
    ADD CONSTRAINT external_identities_id_user_unique UNIQUE (id, user_id),
    ADD COLUMN entitlement_sync_state text NOT NULL DEFAULT 'missing'
        CHECK (entitlement_sync_state IN
            ('missing', 'pending', 'valid', 'invalid', 'legacy_unresolved')),
    ADD COLUMN entitlement_sync_attempt_id uuid,
    ADD COLUMN highest_accepted_entitlement_generation bigint NOT NULL DEFAULT 0
        CHECK (highest_accepted_entitlement_generation >= 0),
    ADD COLUMN last_successful_entitlement_sync_at timestamptz;

ALTER TABLE user_role_assignments
    ADD COLUMN local_grant boolean NOT NULL DEFAULT true,
    ADD COLUMN oidc_identity_id uuid,
    ADD COLUMN legacy_ownership_unresolved boolean NOT NULL DEFAULT false,
    ADD CONSTRAINT user_role_assignment_source CHECK
        (local_grant OR oidc_identity_id IS NOT NULL OR legacy_ownership_unresolved),
    ADD CONSTRAINT user_role_assignment_identity_owner
        FOREIGN KEY (oidc_identity_id, user_id)
        REFERENCES external_identities (id, user_id);

ALTER TABLE user_environment_memberships
    ADD COLUMN local_grant boolean NOT NULL DEFAULT true,
    ADD COLUMN oidc_identity_id uuid,
    ADD COLUMN legacy_ownership_unresolved boolean NOT NULL DEFAULT false,
    ADD CONSTRAINT user_environment_membership_source CHECK
        (local_grant OR oidc_identity_id IS NOT NULL OR legacy_ownership_unresolved),
    ADD CONSTRAINT user_environment_membership_identity_owner
        FOREIGN KEY (oidc_identity_id, user_id)
        REFERENCES external_identities (id, user_id);

UPDATE user_role_assignments assignment
SET local_grant = false, legacy_ownership_unresolved = true
WHERE EXISTS (SELECT 1 FROM external_identities identity
              WHERE identity.user_id = assignment.user_id);

UPDATE user_environment_memberships membership
SET local_grant = false, legacy_ownership_unresolved = true
WHERE EXISTS (SELECT 1 FROM external_identities identity
              WHERE identity.user_id = membership.user_id);

UPDATE external_identities identity
SET entitlement_sync_state = 'legacy_unresolved'
WHERE EXISTS (SELECT 1 FROM user_role_assignments assignment
              WHERE assignment.user_id = identity.user_id
                AND assignment.legacy_ownership_unresolved)
   OR EXISTS (SELECT 1 FROM user_environment_memberships membership
              WHERE membership.user_id = identity.user_id
                AND membership.legacy_ownership_unresolved);

-- No SET NULL: losing an identity must never silently promote its grants to
-- local ownership. Source-aware removal must precede identity deletion.
CREATE INDEX user_role_assignments_oidc_identity_idx
    ON user_role_assignments (oidc_identity_id) WHERE oidc_identity_id IS NOT NULL;
CREATE INDEX user_environment_memberships_oidc_identity_idx
    ON user_environment_memberships (oidc_identity_id) WHERE oidc_identity_id IS NOT NULL;
