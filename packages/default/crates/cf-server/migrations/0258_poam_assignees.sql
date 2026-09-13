-- Add typed POA&M assignees without interpreting existing owner snapshots.
-- The user reference is restrictive so disabled or soft-deleted users remain
-- valid historical identities. Group names are snapshots because an OIDC
-- mapping can be removed without invalidating historical POA&M records.

ALTER TABLE poams
    ADD COLUMN owner_kind text,
    ADD COLUMN owner_user_id uuid REFERENCES users(id) ON DELETE RESTRICT,
    ADD COLUMN owner_group_name text,
    ADD CONSTRAINT poams_owner_assignee_shape CHECK (
        (owner_kind IS NULL AND owner_user_id IS NULL AND owner_group_name IS NULL)
        OR
        (owner_kind = 'user' AND owner_user_id IS NOT NULL AND owner_group_name IS NULL)
        OR
        (owner_kind = 'oidc_group' AND owner_user_id IS NULL AND owner_group_name IS NOT NULL
            AND btrim(owner_group_name) <> '')
    );

-- Build the public assignee view from persisted identity and owner snapshot.
-- Availability is current catalog eligibility; it does not mutate history.
CREATE FUNCTION poam_assignee_view(value poams)
RETURNS jsonb
LANGUAGE sql
STABLE
AS $$
    SELECT CASE value.owner_kind
        WHEN 'user' THEN jsonb_build_object(
            'kind', 'user',
            'user_id', value.owner_user_id,
            'display', value.owner,
            'available', EXISTS (
                SELECT 1 FROM users
                WHERE id = value.owner_user_id
                  AND is_active
                  AND user_type = 'human'
            )
        )
        WHEN 'oidc_group' THEN jsonb_build_object(
            'kind', 'oidc_group',
            'group_name', value.owner_group_name,
            'display', value.owner,
            'available', EXISTS (
                SELECT 1 FROM oidc_group_mappings
                WHERE group_name = value.owner_group_name
            )
        )
        ELSE CASE WHEN value.owner = ''
            THEN jsonb_build_object('kind', 'unassigned')
            ELSE jsonb_build_object('kind', 'legacy', 'display', value.owner)
        END
    END
$$;
