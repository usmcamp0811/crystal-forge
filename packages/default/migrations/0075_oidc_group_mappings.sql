CREATE TABLE IF NOT EXISTS oidc_group_mappings (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    group_name text NOT NULL UNIQUE,
    role auth_role,
    environments text[] NOT NULL DEFAULT ARRAY[]::text[],
    created_at timestamptz NOT NULL DEFAULT NOW(),
    updated_at timestamptz NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_oidc_group_mappings_group_name
    ON oidc_group_mappings (group_name);

CREATE TRIGGER update_oidc_group_mappings_updated_at
    BEFORE UPDATE ON oidc_group_mappings
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
